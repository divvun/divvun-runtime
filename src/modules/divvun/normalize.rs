// use std::borrow::Cow;
// use std::collections::VecDeque;
// use std::io::{self, BufRead, Write};
// use std::sync::Arc;
// use tracing::{debug, error, info, trace, warn};

// #[derive(Debug, Clone)]
// pub struct Output<'a> {
//     buf: Cow<'a, str>,
// }

// #[derive(Debug, Clone)]
// pub enum Line<'a> {
//     WordForm(&'a str),
//     Reading(&'a str),
//     Text(&'a str),
// }

// #[derive(Debug, Clone)]
// pub enum Block<'a> {
//     Cohort(Cohort<'a>),
//     Escaped(&'a str),
//     Text(&'a str),
// }

// #[derive(Clone)]
// pub struct Reading<'a> {
//     pub raw_line: &'a str,
//     pub base_form: &'a str,
//     pub tags: Vec<&'a str>,
//     pub depth: usize,
// }

// impl std::fmt::Debug for Reading<'_> {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         let alt = f.alternate();

//         let mut x = f.debug_struct("Reading");
//         x.field("base_form", &self.base_form)
//             .field("tags", &self.tags)
//             .field("depth", &self.depth);

//         if alt {
//             x.field("raw_line", &self.raw_line).finish()
//         } else {
//             x.finish_non_exhaustive()
//         }
//     }
// }

// #[derive(Debug, Clone)]
// pub struct Cohort<'a> {
//     pub word_form: &'a str,
//     pub readings: Vec<Reading<'a>>,
// }

// #[derive(Debug, thiserror::Error)]
// pub enum Error {
//     #[error("Invalid input: line {line}, position {position}, expected {expected}")]
//     InvalidInput {
//         line: usize,
//         position: usize,
//         expected: &'static str,
//     },
// }

// impl<'a> Output<'a> {
//     pub fn new<S: Into<Cow<'a, str>>>(buf: S) -> Self {
//         let buf = buf.into();
//         Self { buf }
//     }

//     fn lines(&'a self) -> impl Iterator<Item = Line<'a>> {
//         let mut lines = self.buf.lines();
//         std::iter::from_fn(move || {
//             while let Some(line) = lines.next() {
//                 return Some(if line.starts_with('"') {
//                     Line::WordForm(line)
//                 } else if line.starts_with('\t') {
//                     Line::Reading(line)
//                 } else {
//                     Line::Text(line)
//                 });
//             }
//             None
//         })
//     }

//     pub fn iter(&'a self) -> impl Iterator<Item = Result<Block<'a>, Error>> {
//         let mut lines = self.lines().peekable();
//         let mut cohort = None;
//         let mut text = VecDeque::new();

//         std::iter::from_fn(move || loop {
//             if cohort.is_none() {
//                 if let Some(t) = text.pop_front() {
//                     return Some(Ok(t));
//                 }
//             }

//             let Some(line) = lines.peek() else {
//                 if let Some(cohort) = cohort.take() {
//                     return Some(Ok(Block::Cohort(cohort)));
//                 }

//                 return None;
//             };

//             let ret = loop {
//                 match line {
//                     Line::WordForm(x) => {
//                         if let Some(cohort) = cohort.take() {
//                             return Some(Ok(Block::Cohort(cohort)));
//                         }

//                         let (Some(start), Some(end)) = (x.find("\"<"), x.find(">\"")) else {
//                             return Some(Err(Error::InvalidInput { line: 0, position: 0, expected: "WordForm" }));
//                         };

//                         let word_form = &x[start + 2..end];

//                         cohort = Some(Cohort {
//                             word_form,
//                             readings: Vec::new(),
//                         });

//                         break None;
//                     }
//                     Line::Reading(x) => {
//                         let Some(cohort) = cohort.as_mut() else {
//                             break Some(Err(Error::InvalidInput { line: 0, position: 0, expected: "Cohort" }));
//                         };

//                         let Some(depth) = x.rfind('\t') else {
//                             break Some(Err(Error::InvalidInput { line: 0, position: 0, expected: "Depth" }));
//                         };

//                         let x = &x[depth + 1..];
//                         let mut chunks = x.split_ascii_whitespace();

//                         let base_form = match chunks.next().ok_or_else(|| Error::InvalidInput { line: 0, position: 0, expected: "BaseForm" }) {
//                             Ok(v) => v,
//                             Err(e) => break Some(Err(e)),
//                         };

//                         if !(base_form.starts_with('"') && base_form.ends_with('"')) {
//                             break Some(Err(Error::InvalidInput { line: 0, position: 0, expected: "QuotedBaseForm" }));
//                         }
//                         let base_form = &base_form[1..base_form.len() - 1];

//                         cohort.readings.push(Reading {
//                             raw_line: x,
//                             base_form,
//                             tags: chunks.collect(),
//                             depth: depth + 1,
//                         });

//                         break None;
//                     }
//                     Line::Text(x) => {
//                         if x.starts_with(':') {
//                             text.push_back(Block::Escaped(&x[1..]));
//                         } else {
//                             text.push_back(Block::Text(x));
//                         }

//                         break None;
//                     }
//                 }
//             };

//             lines.next();

//             if let Some(ret) = ret {
//                 return Some(ret);
//             }
//         })
//     }
// }

// pub struct Normaliser {
//     normaliser: Arc<HfstTransducer>,
//     generator: Arc<HfstTransducer>,
//     sanalyser: Arc<HfstTransducer>,
//     danalyser: Arc<HfstTransducer>,
//     tags: Vec<String>,
// }

// impl Normaliser {
//     pub fn run<R: BufRead, W: Write>(&self, input: R, mut output: W) -> io::Result<()> {
//         let buffer = input
//             .lines()
//             .map(|line| line.unwrap())
//             .collect::<Vec<String>>()
//             .join("\n");
//         let output_iter = Output::new(buffer);

//         for block in output_iter.iter() {
//             match block {
//                 Ok(Block::Cohort(cohort)) => {
//                     debug!("New surface form: {}", cohort.word_form);
//                     writeln!(output, "{}", format_cohort(&cohort, &self.tags, &*self.normaliser, &*self.generator, &*self.sanalyser))?;
//                 }
//                 Ok(Block::Escaped(escaped)) => {
//                     writeln!(output, " {}", escaped)?;
//                 }
//                 Ok(Block::Text(text)) => {
//                     writeln!(output, "{}", text)?;
//                 }
//                 Err(e) => {
//                     error!("Error: {:?}", e);
//                 }
//             }
//         }

//         Ok(())
//     }
// }

// fn format_cohort<'a>(
//     cohort: &Cohort<'a>,
//     tags: &[String],
//     normaliser: &HfstTransducer,
//     generator: &HfstTransducer,
//     sanalyser: &HfstTransducer,
// ) -> String {
//     let mut results = Vec::new();
//     for reading in &cohort.readings {
//         let mut outstring = String::new();
//         write!(outstring, "{}", reading.raw_line).unwrap();

//         let expand = tags.iter().any(|tag| outstring.contains(tag));

//         let surf = if expand {
//             expand_readings(reading, tags, normaliser, generator, sanalyser)
//         } else {
//             outstring.clone()
//         };

//         results.push(surf);
//     }

//     results.join("\n")
// }

// fn expand_readings<'a>(
//     reading: &Reading<'a>,
//     tags: &[String],
//     normaliser: &HfstTransducer,
//     generator: &HfstTransducer,
//     sanalyser: &HfstTransducer,
// ) -> String {
//     let mut results = Vec::new();
//     let surf = reading.base_form.to_string();
//     debug!("1. looking up normaliser");

//     let expansions = normaliser.lookup_fd(&surf, -1, 2.0);
//     for e in expansions.iter() {
//         let phon = e.to_string();
//         let newlemma = e.to_string();
//         let mut reanal = reading.tags.join(" ");
//         let regen = format!("{}{}", e, tags.iter().collect::<String>());

//         debug!("2.a Using normalised form: {}", regen);

//         let regenerations = generator.lookup_fd(&regen, -1, 2.0);
//         for rg in regenerations.iter() {
//             let reanalyses = sanalyser.lookup_fd(&phon, -1, 2.0);
//             for ra in reanalyses.iter() {
//                 reanal = ra.to_string();
//                 reanal = reanal[reanal.find('+').unwrap()..].to_string();
//                 reanal = reanal.replace('+', " ");
//                 results.push(format!("{}\"{}\" {} \"{}\" {}oldlemma", reading.depth, newlemma, reanal, phon, surf));
//             }
//         }

//         if results.is_empty() {
//             results.push(outstring.clone());
//         }
//     }

//     results.join("\n")
// }

// // Placeholder trait and struct definitions for HfstTransducer.

// type HfstPaths1L = Vec<String>;

// pub struct HfstTransducer;

// impl HfstTransducer {
//     pub fn lookup_fd(&self, word: &str, _a: i32, _b: f64) -> HfstPaths1L {
//         // Placeholder implementation
//         vec![word.to_string()]
//     }
// }

// fn main() {
//     tracing_subscriber::fmt::init();

//     // Example usage
//     let input_data = r#"
//         "<Wikipedia>"
//         "Wikipedia" Err/Orth N Prop Sem/Org Attr <W:0.0>
//         "Wikipedia" Err/Orth N Prop Sem/Org Sg Acc <W:0.0>
//         "Wikipedia" Err/Orth N Prop Sem/Org Sg Gen <W:0.0>
//         "Wikipedia" Err/Orth N Prop Sem/Org Sg Nom <W:0.0>
//         "Wikipedia" N Prop Sem/Org Attr <W:0.0>
//         "Wikipedia" N Prop Sem/Org Sg Acc <W:0.0>
//         "Wikipedia" N Prop Sem/Org Sg Gen <W:0.0>
//         "Wikipedia" N Prop Sem/Org Sg Nom <W:0.0>
//         :
//         "<lea>"
//         "leat" V IV Ind Prs Sg3 <W:0.0>
//         :
//         "<friddja>"
//         "friddja" A Sem/Hum Attr <W:0.0>
//         "friddja" A Sem/Hum Sg Acc <W:0.0>
//         "friddja" A Sem/Hum Sg Gen <W:0.0>
//         "friddja" A Sem/Hum Sg Nom <W:0.0>
//         "friddja" Adv <W:0.0>
//         :
//         "<diehtosátnegirji>"
//         "sátnegirji" N Sem/Txt Sg Nom <W:0.0>
//                 "diehtu" N Sem/Prod-cogn_Txt Cmp/SgNom Cmp/SoftHyph Err/Orth Cmp <W:0.0>
//         "girji" N Sem/Txt Sg Nom <W:0.0>
//                 "sátni" N Sem/Cat Cmp/SgNom Cmp <W:0.0>
//                         "diehtu" N Sem/Prod-cogn_Txt Cmp/SgNom Cmp/SoftHyph Err/Orth Cmp <W:0.0>
//         "sátnegirji" N Sem/Txt Sg Nom <W:0.0>
//                 "dihto" A Err/Orth Sem/Dummytag Cmp/Attr Cmp/SoftHyph Err/Orth Cmp <W:0.0>
//         "girji" N Sem/Txt Sg Nom <W:0.0>
//                 "sátni" N Sem/Cat Cmp/SgNom Cmp <W:0.0>
//                         "dihto" A Err/Orth Sem/Dummytag Cmp/Attr Cmp/SoftHyph Err/Orth Cmp <W:0.0>
//         :
//         "<badjel>"
//         "badjel" Adv Sem/Plc <W:0.0>
//         "badjel" Adv Sem/Plc Gen <W:0.0>
//         "badjel" Po <W:0.0>
//         "badjel" Pr <W:0.0>
//         :
//         "<300>"
//         "300" Num Arab Sg Acc <W:0.0>
//         "300" Num Arab Sg Gen <W:0.0>
//         "300" Num Arab Sg Ill Attr <W:0.0>
//         "300" Num Arab Sg Loc Attr <W:0.0>
//         "300" Num Arab Sg Nom <W:0.0>
//         "300" Num Sem/ID <W:0.0>
//         :
//         "<gielainn>"
//         "gielainn" ?
//         "<.>"
//         "." CLB <W:0.0>
//         :
//     "#;

//     let normaliser = Normaliser {
//         normaliser: Arc::new(HfstTransducer {}),
//         generator: Arc::new(HfstTransducer {}),
//         sanalyser: Arc::new(HfstTransducer {}),
//         danalyser: Arc::new(HfstTransducer {}),
//         tags: vec!["Err/Orth".to_string(), "Cmpnd".to_string()],
//     };

//     let input = input_data.as_bytes();
//     let mut output = Vec::new();

//     normaliser.run(input, &mut output).unwrap();
//     println!("{}", String::from_utf8(output).unwrap());
// }

use syntax_highlight::{highlight_to_terminal, supports_color};

fn main() {
    let json_sample = r#"{
  "name": "divvun-runtime",
  "version": "0.2.2",
  "features": ["mod-hfst", "mod-cg3"]
}"#;

    let cg3_sample = r#";"<word>"
	"lemma" TAG1 TAG2
	"other" TAG3
:REPLACE FORM
"<another>"
	"test" N Sg"#;

    println!("Color support: {}\n", supports_color());

    println!("JSON highlighting:");
    println!("{}\n", highlight_to_terminal(json_sample, "json"));

    println!("CG3 highlighting:");
    println!("{}", highlight_to_terminal(cg3_sample, "cg3"));
}

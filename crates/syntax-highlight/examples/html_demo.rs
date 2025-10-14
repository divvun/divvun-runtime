use syntax_highlight::highlight_to_html;

fn main() {
    let json_sample = r#"{
  "name": "divvun-runtime",
  "version": "0.2.2"
}"#;

    let html = highlight_to_html(json_sample, "json");
    println!("HTML output:");
    println!("{}", html);
}

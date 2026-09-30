//! Ask a bundle's cgspell acceptor directly whether each word is an entry, as
//! DRT_Bundle_isCorrect does. Quote multi-word entries: "dan dihte".
//!
//! Usage: cargo run --release --example cgspell_is_correct -- <bundle.drb> <word>...

use divvun_runtime::bundle::Bundle;
use divvun_runtime::modules::divvun::Cgspell;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let bundle_path = args
        .next()
        .ok_or("usage: cgspell_is_correct <bundle.drb> <word>...")?;

    let bundle = Bundle::from_bundle(&bundle_path).await?;
    let (_, cgspell) = bundle
        .command::<Cgspell>(None)
        .ok_or("no cgspell command in bundle")?;

    for word in args {
        println!("{word:?}\t{}", cgspell.is_correct(&word));
    }

    Ok(())
}

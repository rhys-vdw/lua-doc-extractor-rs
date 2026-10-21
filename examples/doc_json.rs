//! Prints emmylua's understanding of a directory of Lua files as JSON.
//!
//! cargo run --example doc_json -- <dir>

use emmylua_doc_cli::{CmdArgs, Format, OutputDestination, run_doc_cli};

fn main() {
    let dir = std::env::args().nth(1).expect("usage: doc_json <dir>");
    run_doc_cli(CmdArgs {
        config: None,
        workspace: vec![dir.into()],
        exclude_pattern: None,
        include_pattern: None,
        output_format: Format::Json,
        output: OutputDestination::Stdout,
        override_template: None,
        site_name: None,
        mixin: None,
        verbose: false,
    })
    .expect("emmylua_doc_cli failed");
}

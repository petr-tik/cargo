use crate::command_prelude::*;

use cargo::{
    core::{Target, Workspace},
    ops::CompileOptions,
    util::{errors::CargoResult, get_available_targets},
};
use skim::{self, prelude::*};
use std::io::Cursor;

pub fn cli() -> App {
    subcommand("query")
        .about("List query targets")
        .after_help("Run `cargo help query` for more detailed information.\n")
}

enum QueryTargets {
    // Find all buildable binary executable
    Binaries,
    // Find all examples in the workspace
    // REVIEW should examples also be present in Binaries?
    Examples,
    // Find all the tests
    // REVIEW Target::is_test finds only integration tests, I want to list all test targets
    // REVIEW rust-analyzer reuse for finding runnables
    Tests,
    // Find all features defined in this workspace to help complete
    // --features <TAB><TAB>
    Features,
}

// TODO generalise it to tests that can be
// specified and cargo run that takes
// cargo query --bin $SPECIFIC_BINARY_TARGET
// cargo query --example $SPECIFIC_EXAMPLE

fn choose_target(
    ws: &Workspace<'_>,
    compile_opts: &CompileOptions,
) -> CargoResult<Vec<Arc<dyn SkimItem>>> {
    let targets = get_available_targets(Target::is_bin, &ws, &compile_opts)?;

    // pass string representations of targets to skim
    let input = targets.join("\n");

    // `SkimItemReader` is a helper to turn any `BufRead` into a stream of `SkimItem`
    // `SkimItem` was implemented for `AsRef<str>` by default
    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(Cursor::new(input));

    let skim_options_for_single_selection = SkimOptionsBuilder::default()
        .height(Some("70%"))
        // TODO condition on the input
        .multi(false)
        .build()
        .unwrap();

    // TODO can only build 1 target, if not error with nothing
    let selected_items: Vec<Arc<dyn SkimItem>> =
        Skim::run_with(&skim_options_for_single_selection, Some(items))
            .map(|out| out.selected_items)
            .unwrap_or_else(|| Vec::new());

    Ok(selected_items)
}

pub fn exec(config: &mut Config, args: &ArgMatches<'_>) -> CliResult {
    let ws = args.workspace(config)?;

    let compile_opts = args.compile_options(
        config,
        CompileMode::Build,
        Some(&ws),
        ProfileChecking::Custom,
    )?;

    let target = choose_target(&ws, &compile_opts)?;

    // TODO 3 compile the selected target - demo-only
    // the final query command will only create skim/fzf selecting UIs
    // and the bash plumbing will forward the selection to the command line
    // ops::compile(&ws, &compile_opts)?;

    println!("{:?}", target[0].text());

    Ok(())
}

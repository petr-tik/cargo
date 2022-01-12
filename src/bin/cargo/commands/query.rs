use crate::command_prelude::*;

use cargo::{
    core::{compiler::CompileKind, Target, Workspace},
    ops::CompileOptions,
    util::{errors::CargoResult, get_available_targets},
};
use clap::{arg_enum, value_t_or_exit};
use skim::{self, prelude::*};
use std::io::Cursor;

pub fn cli() -> App {
    subcommand("query")
        .arg(
            Arg::with_name("type")
                .help("What are we querying for")
                .possible_values(&QueryTargets::variants())
                .case_insensitive(true),
        )
        .about("List query targets")
        .after_help("Run `cargo help query` for more detailed information.\n")
}

arg_enum! {
pub enum QueryTargets {
    // Find all buildable binary executable
    Binaries,
    // Find all examples in the workspace
    // REVIEW should examples also be present in Binaries?
    Examples,
    // Find all the tests
    // REVIEW Target::is_test finds only integration tests, I want to list all test targets
    // REVIEW rust-analyzer reuse for finding runnables
    Tests,
    // Find all benchmark build targets
    Benches,
    // Find all features defined in this workspace to help complete
    // --features <TAB><TAB>
    Features,
    // Build profile
    Profile,
}
}

impl QueryTargets {
    fn as_target_pred(&self) -> fn(&Target) -> bool {
        match self {
            QueryTargets::Binaries => Target::is_bin,
            QueryTargets::Tests => Target::is_test,
            QueryTargets::Benches => Target::is_bench,
            QueryTargets::Examples => Target::is_example,
            QueryTargets::Features | QueryTargets::Profile => unimplemented!(),
        }
    }

    fn allows_multi(&self) -> bool {
        match self {
            QueryTargets::Binaries
            | QueryTargets::Examples
            | QueryTargets::Tests
            | QueryTargets::Benches
            | QueryTargets::Profile => false,
            QueryTargets::Features => true,
        }
    }
}

impl From<&QueryTargets> for CompileMode {
    fn from(val: &QueryTargets) -> Self {
        match val {
            QueryTargets::Binaries | QueryTargets::Examples => CompileMode::Build,
            QueryTargets::Tests => CompileMode::Test,
            QueryTargets::Benches => CompileMode::Bench,
            QueryTargets::Features | QueryTargets::Profile => unimplemented!(),
        }
    }
}

fn choose_target(
    ws: &Workspace<'_>,
    compile_opts: &CompileOptions,
    query_target: QueryTargets,
) -> CargoResult<Vec<Arc<dyn SkimItem>>> {
    let targets = get_available_targets(query_target.as_target_pred(), &ws, &compile_opts)?;

    // pass string representations of targets to skim
    let input = targets.join("\n");

    // `SkimItemReader` is a helper to turn any `BufRead` into a stream of `SkimItem`
    // `SkimItem` was implemented for `AsRef<str>` by default
    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(Cursor::new(input));

    let skim_options = SkimOptionsBuilder::default()
        .height(Some("70%"))
        .multi(query_target.allows_multi())
        .build()
        .unwrap();

    // TODO can only build 1 target, if not error with nothing
    let selected_items: Vec<Arc<dyn SkimItem>> = Skim::run_with(&skim_options, Some(items))
        .map(|out| out.selected_items)
        .unwrap_or_else(|| Vec::new());

    Ok(selected_items)
}

pub fn exec(config: &mut Config, args: &ArgMatches<'_>) -> CliResult {
    let query_target = value_t_or_exit!(args.value_of("type"), QueryTargets);
    let ws = args.workspace(config)?;

    let compile_opts = args.compile_options(
        config,
        (&query_target).into(),
        Some(&ws),
        ProfileChecking::Custom,
    )?;

    let target = choose_target(&ws, &compile_opts, query_target)?;

    println!("{:?}", target[0].text());

    Ok(())
}

use crate::command_prelude::*;

use cargo::{
    core::{profiles::Profiles, Target, Workspace},
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

// REVIEW doesn't look like this macro supports 2 nested enums
// Buildable and ProjectConfig (Features, Profiles)
arg_enum! {
pub enum QueryTargets {
    // Find all buildable binary executable
    Binaries,
    // Find all examples in the workspace
    Examples,
    // Find all the tests
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
            // REVIEW Target::is_test finds only integration tests, I want to list all test targets
            // REVIEW rust-analyzer reuse for finding runnables
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

    fn make_skim_inputs(
        &self,
        ws: &Workspace<'_>,
        compile_opts: &CompileOptions,
    ) -> CargoResult<String> {
        // REVIEW can I get all available Profiles somehow without passing a requested_profile?
        let profs = Profiles::new(ws, compile_opts.build_config.requested_profile)?;
        let targets = match self {
            QueryTargets::Binaries
            | QueryTargets::Examples
            | QueryTargets::Tests
            | QueryTargets::Benches => {
                get_available_targets(self.as_target_pred(), &ws, &compile_opts)?
            }
            QueryTargets::Profile => get_available_profiles(&profs)?,
            QueryTargets::Features => unimplemented!(),
        };

        // pass string representations of targets to skim
        Ok(targets.join("\n"))
    }
}

impl From<&QueryTargets> for CompileMode {
    fn from(val: &QueryTargets) -> Self {
        match val {
            // REVIEW this might need to come from another argument to query
            // eg. cargo build <TAB><TAB> might be different from
            // cargo run --features <TAB><TAB>
            QueryTargets::Binaries | QueryTargets::Examples => CompileMode::Build,
            QueryTargets::Tests => CompileMode::Test,
            QueryTargets::Benches => CompileMode::Bench,
            // HACK will be removed once QueryTargets is split into Buildable and ProjectConfigs
            QueryTargets::Profile => CompileMode::Build,
            QueryTargets::Features => unimplemented!(),
        }
    }
}

fn get_available_profiles<'a>(profs: &'a Profiles) -> CargoResult<Vec<&'a str>> {
    let res = profs.list_all();
    Ok(res)
}

fn fuzzy_choose(
    ws: &Workspace<'_>,
    compile_opts: &CompileOptions,
    query_target: QueryTargets,
) -> CargoResult<Vec<Arc<dyn SkimItem>>> {
    let input = query_target.make_skim_inputs(ws, compile_opts)?;

    let items = SkimItemReader::default().of_bufread(Cursor::new(input));

    let skim_options = SkimOptionsBuilder::default()
        .height(Some("70%"))
        .multi(query_target.allows_multi())
        .build()
        .unwrap();

    // TODO return selection, otherwise bail
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

    let target = fuzzy_choose(&ws, &compile_opts, query_target)?;

    let out_as_bytes = target[0].text().to_owned();

    config.shell().print_ansi_stdout(out_as_bytes.as_bytes())?;

    Ok(())
}

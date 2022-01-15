use crate::command_prelude::*;

use anyhow::format_err;
use cargo::{
    core::{profiles::Profiles, Target, Workspace},
    ops::CompileOptions,
    util::{errors::CargoResult, get_available_targets},
};
use clap::{ArgEnum, PossibleValue};
use itertools::join;
use skim::{self, prelude::*};
use std::io::Cursor;
use std::{fmt::Display, str::FromStr};

// REVIEW doesn't look like this macro supports 2 nested enums
// Buildable and ProjectConfig (Features, Profiles)
#[derive(clap::ArgEnum, Clone, Debug)]
enum QueryTargets {
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

impl QueryTargets {
    pub fn possible_values() -> impl Iterator<Item = PossibleValue<'static>> {
        QueryTargets::value_variants()
            .iter()
            .filter_map(ArgEnum::to_possible_value)
    }
}

impl FromStr for QueryTargets {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> CargoResult<Self> {
        match s {
            "tests" => Ok(QueryTargets::Tests),
            "binaries" => Ok(QueryTargets::Binaries),
            "examples" => Ok(QueryTargets::Examples),
            "benches" => Ok(QueryTargets::Benches),
            "features" => Ok(QueryTargets::Features),
            "profiles" => Ok(QueryTargets::Profile),
            _ => Err(anyhow::format_err!("Unknown type {}", s)),
        }
    }
}

pub fn cli() -> App {
    subcommand("query")
        .arg(
            Arg::new("type")
                .help("What are we querying for")
                .possible_values(QueryTargets::possible_values())
                .ignore_case(true),
        )
        .about("List query targets")
        .after_help("Run `cargo help query` for more detailed information.\n")
        // TODO all these below are hacks around the fact that
        // `ArgMatches::compile_options` retrieves all of those fields
        // under the hood
        .arg_message_format()
        .arg_jobs()
        .arg_features()
        .arg_target_triple("Placeholder to make the damn thing compile")
        .arg_profile("Placeholder to make the damn thing compile")
        .arg_targets_all(
            "Placeholder to make the damn thing compile",
            "Placeholder to make the damn thing compile",
            "Placeholder to make the damn thing compile",
            "Placeholder to make the damn thing compile",
            "Placeholder to make the damn thing compile",
            "Placeholder to make the damn thing compile",
            "Placeholder to make the damn thing compile",
            "Placeholder to make the damn thing compile",
            "Placeholder to make the damn thing compile",
            "Placeholder to make the damn thing compile",
        )
}

impl Display for QueryTargets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

struct MySkimOptions<'a> {
    allows_multi: bool,
    prompt: Option<&'a str>,
    input: String,
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
            QueryTargets::Features | QueryTargets::Profile => {
                unimplemented!(
                    "You shouldn't be filtering build targets with {:?}",
                    self.to_string()
                )
            }
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

    fn make_skim_options(
        &self,
        ws: &Workspace<'_>,
        compile_opts: &CompileOptions,
    ) -> CargoResult<MySkimOptions<'_>> {
        // REVIEW can I get all Profiles available in the workspace somehow without passing a requested_profile?
        // ws.profiles() returned None when I ran it
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
        Ok(MySkimOptions {
            input: targets.join("\n"),
            // TODO Customise the prompt with the `QueryTargets` .to_string() representation
            prompt: Some("Choose> "),
            allows_multi: self.allows_multi(),
        })
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
    let options = query_target.make_skim_options(ws, compile_opts)?;

    let items = SkimItemReader::default().of_bufread(Cursor::new(options.input));

    let skim_options = SkimOptionsBuilder::default()
        // TODO move to a field in MySkimOptions that can be heuristically set.
        // makes now sense to have a massive window for a handful of candidates
        .height(Some("40%"))
        .multi(options.allows_multi)
        .prompt(options.prompt)
        .build()
        .unwrap();

    // TODO return selection, otherwise bail
    let selected_items = Skim::run_with(&skim_options, Some(items)).unwrap();
    match selected_items.final_event {
        // REVIEW Maybe replace with Some/None
        Event::EvActAbort => return Err(format_err!("Aborted without selecting anything").into()),
        Event::EvActAccept(_) => Ok(selected_items.selected_items),
        _ => unimplemented!(),
    }
}

fn convert_selected_items_to_string(items: Vec<Arc<dyn SkimItem>>) -> CargoResult<String> {
    Ok(join(items.iter().map(|i| i.text()), ","))
}

pub fn exec(config: &mut Config, args: &ArgMatches) -> CliResult {
    let query_target: QueryTargets = args.value_of_t_or_exit("type");

    let ws = args.workspace(config)?;
    let compile_opts = args.compile_options(
        config,
        (&query_target).into(),
        Some(&ws),
        ProfileChecking::Custom,
    )?;

    match convert_selected_items_to_string(fuzzy_choose(&ws, &compile_opts, query_target)?) {
        Ok(it) => config.shell().print_ansi_stdout(it.as_bytes())?,
        Err(_) => (),
    };

    Ok(())
}

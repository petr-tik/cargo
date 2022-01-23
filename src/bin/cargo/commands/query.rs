use crate::command_prelude::*;

use cargo::{
    core::{profiles::Profiles, Target, Workspace},
    ops::CompileOptions,
    util::{errors::CargoResult, get_available_targets},
};
use clap::{ArgEnum, PossibleValue};
use itertools::join;
use skim::{self, prelude::*};
use std::io::Cursor;
use std::str::FromStr;

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

impl AsRef<str> for QueryTargets {
    fn as_ref(&self) -> &str {
        match self {
            QueryTargets::Binaries => "binaries",
            QueryTargets::Examples => "examples",
            QueryTargets::Tests => "tests",
            QueryTargets::Benches => "benches",
            QueryTargets::Features => "features",
            QueryTargets::Profile => "profile",
        }
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
            "profile" => Ok(QueryTargets::Profile),
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

type NewLineSeparatedInput = String;

struct MySkimOptions<'a> {
    /// Can the user choose multiple options
    allows_multi: bool,
    /// A substring that specifies what the user will choose on the UI
    prompt: &'a str,
    /// Input to present to the user
    // REVIEW make a &str if possible
    input: NewLineSeparatedInput,
    /// Absolute height (in lines) of the UI
    // TODO add a heuristic to calc height as percentage
    // makes no sense to have a massive window for a handful of candidates
    abs_height: usize,
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
                    self.as_ref()
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
            prompt: self.as_ref(),
            allows_multi: self.allows_multi(),
            abs_height: targets.len() * 3,
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

    let abs_height = format!("{}", options.abs_height);
    let full_prompt = format!("Choose {}> ", options.prompt);

    let skim_options = SkimOptionsBuilder::default()
        .prompt(Some(&full_prompt))
        .height(Some(&abs_height))
        .multi(options.allows_multi)
        .build()
        .unwrap();

    let items = SkimItemReader::default().of_bufread(Cursor::new(options.input));

    let skim_res = match Skim::run_with(&skim_options, Some(items)) {
        Some(res) => Ok(res),
        None => Err(anyhow::format_err!("Internal skim error")),
    }?;

    match skim_res.final_event {
        Event::EvActAccept(_) => Ok(skim_res.selected_items),
        // TODO bring back proper error handling Err(format_err!("Aborted without selecting anything").into())
        _ => Ok(vec![]),
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

    if let Ok(it) =
        convert_selected_items_to_string(fuzzy_choose(&ws, &compile_opts, query_target)?)
    {
        config.shell().print_ansi_stdout(it.as_bytes())?
    };

    Ok(())
}

use crate::{core::Workspace, CargoResult};

pub fn query(_ws: &Workspace<'_>) -> CargoResult<()> {
    // TODO move the logic from fuzzy.rs::exec to here
    // make this function return the build target/compile options
    // the exec can then pass it to
    // ops::compile
    // in commands/fuzzy.rs::exec
    Ok(())
}

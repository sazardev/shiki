//! A pluggable "what day is it" capability — `SystemClock` just reads the
//! host's local clock (`chrono::Local::now()`), the same source every
//! `chrono::Local::now()` call site across this crate (`daily.rs`,
//! `note.rs`, `notebook.rs`, `tasks.rs`, `trash.rs`, `query.rs`, …) already
//! uses directly today. This trait is *not* wired into any of those call
//! sites yet — unlike `vcs::VcsPort`/`fs::FileStore`, nothing here is a
//! portability blocker (`chrono::Local::now()` compiles and runs fine on
//! every target this crate already builds for, `wasm32-unknown-unknown`
//! included), so rewriting a baker's dozen call sites for a capability with
//! no concrete consumer yet would be exactly the speculative abstraction
//! this codebase's own conventions warn against. It exists now so a real
//! future need (deterministic tests beyond what `tempfile`-based fixtures
//! already cover, or a server handling client-supplied timezones) has
//! somewhere to plug in without inventing the pattern from scratch — follow
//! the same "trait + `Native*`/`System*` default, `_with_clock` twin (or an
//! injected field, wherever there's already a `&self`)" shape every other
//! port in this crate uses (see `fs.rs`/`vcs.rs`'s own module docs) once
//! that need actually shows up.

pub trait Clock: Send + Sync {
    fn today(&self) -> chrono::NaiveDate;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn today(&self) -> chrono::NaiveDate {
        chrono::Local::now().date_naive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_returns_todays_date() {
        assert_eq!(SystemClock.today(), chrono::Local::now().date_naive());
    }
}

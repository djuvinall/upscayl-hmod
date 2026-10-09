//! Where a path points. Nothing here opens anything.
//!
//! **This is the file P063 exists for**, so it says the whole rule in one place:
//!
//! * a **relative** path resolves against the *base directory* -- the folder of the open
//!   `.hollow` project, or the folder of the executable HollowDeck was launched from
//!   when no project is open (`docs/plans/PLANNER.md`, consideration 7). The core computes it
//!   and hands it over on every call, because the open project can change while this
//!   module runs;
//! * an **absolute** path is used exactly as given;
//! * `..` is allowed, and is resolved **lexically** -- `a/b/../c` is `a/c` whether or not
//!   `a/b` exists, and without asking the filesystem what `b` really is;
//! * **there is no containment.** P049's held walk is gone for v1.0 and the reasoning is
//!   in `decisions.md` (2026-09-20, P063). Permissions are v2.0 (G7).
//!
//! What is left is not a boundary, it is spelling: the refusals below exist because the
//! path could not be turned into one path, not because it pointed somewhere unwelcome.
//!
//! **Two spellings are taken off a path before anything else looks at it**, and both
//! cost something worth stating rather than leaving to be found:
//!
//! * **surrounding whitespace**, so a file whose name really begins or ends with a
//!   space is not reachable through this socket;
//! * **one matching pair of surrounding quotes** (P090), so a file whose name really
//!   begins *and* ends with the same quote character is not reachable either -- quote
//!   it a second time and it is, which is why only one pair comes off. That trade buys
//!   the thing a person actually does: Windows Explorer's *Copy as path* (Ctrl+Shift+C)
//!   puts `"F:\temp\a.txt"` on the clipboard, quotes included, and a string
//!   beginning with `"` is not absolute -- so it was joined onto the base directory and
//!   the refusal read as though this module had refused to leave the project folder,
//!   which is the one thing it does not do.

use std::path::{Component, Path, PathBuf};

/// The whole of what a relative path resolves against, plus how the core decided it.
///
/// `None` is *"nobody told this module what a relative path means"* -- a tool called
/// on the module's own port rather than through the core's broker, or a core too old to
/// inject one. A relative path is then refused with that sentence rather than resolved
/// against this process's working directory, which is a directory the module was given
/// by whoever spawned it and means nothing to the person who typed `notes/a.txt`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Base {
    pub dir: Option<PathBuf>,
    pub source: Option<String>,
}

impl Base {
    pub fn of(dir: &str, source: &str) -> Base {
        let path = PathBuf::from(dir);
        Base {
            dir: (!dir.is_empty() && path.is_absolute()).then_some(path),
            source: (!source.is_empty()).then(|| source.to_string()),
        }
    }
}

/// Resolve one path input. The returned path is absolute and lexically normalised; it
/// may or may not exist, which is the caller's question and not this one's.
///
/// `what` names the socket in a refusal, so a graph author is told which box was wrong.
pub fn resolve(base: &Base, raw: &str, what: &str) -> Result<PathBuf, String> {
    // Surrounding whitespace goes, because a path that arrives on a wire usually carries
    // the newline of whatever produced it and `"notes.txt\n"` is a different file from
    // `"notes.txt"` on POSIX -- one that never exists. A file whose name really begins
    // or ends with a space is not reachable through this socket, which is the trade.
    //
    // Then the quotes, then the whitespace again -- that order, for a reason. The value
    // arrives off a clipboard *and* over a wire, so it can carry both, and which is
    // outermost depends on which happened last. Trimming first is what exposes the
    // quotes to `unquote` at all; trimming again catches the space a person left just
    // inside them. Unquoting twice is what is deliberately not done.
    let text = unquote(raw.trim()).trim();
    if text.is_empty() {
        return Err(format!("{what} is empty: there is no file to point at"));
    }
    let given = Path::new(text);
    if let Some(problem) = drive_relative(given) {
        return Err(format!("{what} {text:?} {problem}"));
    }
    let joined = if given.is_absolute() {
        given.to_path_buf()
    } else {
        let Some(dir) = base.dir.as_ref() else {
            return Err(format!(
                "{what} {text:?} is relative, and nothing said what it is relative to. \
                 A relative path resolves against the open project's folder, or the \
                 folder HollowDeck was launched from -- and the core is what knows \
                 which. Call this tool through the core's broker, or give an absolute \
                 path."
            ));
        };
        // `join` is not blind concatenation: on Windows a rooted-but-driveless path
        // (`\notes\a.txt`) keeps the base's drive, which is what that spelling means to
        // the operating system.
        dir.join(given)
    };
    Ok(normalise(&joined))
}

/// Take off **one** matching pair of surrounding quotes, and only when the string both
/// starts and ends with the same one.
///
/// `'a.txt'` and `"a.txt"` unquote. `"a.txt'` does not, because the ends disagree and
/// a guess about which one was meant is a guess about which file. `"a.txt` does not, for
/// the same reason. `a"b.txt` is untouched -- this never looks inside, and a quote in the
/// middle of a name is an ordinary character on POSIX. A lone `"` is untouched too: one
/// character cannot be a pair, which `next_back` on an already-advanced iterator gets
/// right with no length check of its own.
fn unquote(text: &str) -> &str {
    let mut chars = text.chars();
    match (chars.next(), chars.next_back()) {
        (Some(open @ ('"' | '\'')), Some(close)) if close == open => chars.as_str(),
        _ => text,
    }
}

/// How [`resolve`] treated a path, in one sentence, for the refusal that comes *after*
/// it: a path that resolved fine and then did not open.
///
/// **This exists because a resolved path alone is not an explanation** (P090). One
/// backslash in twenty is the difference between "you gave me an absolute path and I used
/// it" and "you gave me a relative path and I joined it to somewhere you were not
/// thinking of", and a person reading `...\target\debug\"F:\temp\a.txt"` reasonably
/// concludes the module refused to leave the project folder -- which is the one thing it
/// does not do. So the message says which of the two happened, in words, and names the
/// folder when there is one to name.
///
/// It re-derives the spelling rules rather than taking the resolved path, so that the
/// text it quotes back is the path *after* trimming and unquoting: that is what was
/// looked for, and showing anything else would hide the very thing that moved.
///
/// **It is kept short on purpose.** The core's broker carries a module's refusal to the
/// caller with the body cut at a fixed length, so a sentence that runs long is a sentence
/// that arrives half-finished -- which is the failure this plan is about, one layer up.
/// The raw text is quoted with plain quotes rather than `{:?}` for the same reason a
/// person cares about here: `{:?}` doubles every backslash, and a Windows path printed
/// with twice the backslashes it has is the opposite of readable.
pub fn treatment(base: &Base, raw: &str, what: &str) -> String {
    let text = unquote(raw.trim()).trim();
    if Path::new(text).is_absolute() {
        return format!("{what} \"{text}\" is an absolute path and was used exactly as given");
    }
    let Some(dir) = base.dir.as_ref() else {
        // `resolve` has already refused this and said so at length; the sentence still
        // has to be a sentence if anything reaches for it.
        return format!(
            "{what} \"{text}\" is a relative path, and nothing said what it is relative to"
        );
    };
    let from = match base.source.as_deref() {
        Some("project") => " (the open project's folder)",
        Some("executable") => " (the folder HollowDeck was launched from)",
        // A source this module has not heard of: name the folder and stop, rather than
        // dress up a word the core invented after this copy was taken.
        Some(_) | None => "",
    };
    format!(
        "{what} \"{text}\" is a relative path, joined to {}{from}. An absolute path is \
         used exactly as given, wherever it points",
        dir.display()
    )
}

/// `C:notes.txt` -- a drive letter with no separator after it. Windows reads that as
/// *"relative to whatever directory that drive is currently on"*, a per-process, per-drive
/// piece of state this module does not have and would not want. It is refused rather
/// than silently treated as `C:\notes.txt`, which is a different file.
fn drive_relative(path: &Path) -> Option<&'static str> {
    let mut components = path.components();
    match (components.next(), components.next()) {
        (Some(Component::Prefix(prefix)), next)
            if matches!(
                prefix.kind(),
                std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)
            ) && !matches!(next, Some(Component::RootDir)) =>
        {
            Some(
                "names a drive but no directory on it (`C:notes.txt`), which means \
                 \"wherever that drive happens to be\" -- write `C:\\notes.txt` for the \
                 root of the drive, or `notes.txt` for the base directory",
            )
        }
        _ => None,
    }
}

/// Resolve `.` and `..` **without touching the filesystem**, and without turning the
/// path into a verbatim (`\\?\`) one.
///
/// Two deliberate choices:
///
/// * **`Path::canonicalize` is not used.** It fails for a file that does not exist yet,
///   which is every `write_file`, and on Windows it returns a `\\?\` verbatim path --
///   `bundle.rs` learned the hard way that handing one of those to another program
///   changes that program's behaviour (`decisions.md`, 2026-09-19). What comes out of
///   here is what a person would have typed.
/// * **A verbatim path is left alone.** `\\?\` means *"do not normalise this"* to the
///   operating system, so normalising it here would be this module overruling the one
///   spelling that exists to say do not.
fn normalise(path: &Path) -> PathBuf {
    if path
        .components()
        .next()
        .is_some_and(|c| matches!(c, Component::Prefix(p) if p.kind().is_verbatim()))
    {
        return path.to_path_buf();
    }
    let mut out = PathBuf::new();
    let mut rooted = false;
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // `..` above the root is the root: `C:\..` is `C:\`, and `/..` is `/`,
                // which is what every shell answers too. `PathBuf::pop` returns false
                // exactly there -- and in a *relative* path with nothing left to pop,
                // where `..` is a real component that has to stay.
                if !out.pop() && !rooted {
                    out.push(component.as_os_str());
                }
            }
            other => {
                rooted |= matches!(other, Component::RootDir | Component::Prefix(_));
                out.push(other.as_os_str());
            }
        }
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Base {
        Base::of(if cfg!(windows) { "C:\\base" } else { "/base" }, "project")
    }

    fn want(path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    #[test]
    fn a_relative_path_resolves_against_the_base_directory() {
        let got = resolve(&base(), "notes/a.txt", "path").expect("resolved");
        assert_eq!(
            got,
            want(if cfg!(windows) {
                "C:\\base\\notes\\a.txt"
            } else {
                "/base/notes/a.txt"
            })
        );
    }

    /// The reversal, in one assertion: P049 refused this outright.
    #[test]
    fn an_absolute_path_is_used_as_given() {
        let absolute = if cfg!(windows) {
            "F:\\temp\\thing.txt"
        } else {
            "/tmp/thing.txt"
        };
        assert_eq!(
            resolve(&base(), absolute, "path").expect("resolved"),
            want(absolute)
        );
    }

    /// ...and so is this one.
    #[test]
    fn dot_dot_is_allowed_and_resolved_lexically() {
        let got = resolve(&base(), "a/b/../c.txt", "path").expect("resolved");
        assert_eq!(
            got,
            want(if cfg!(windows) {
                "C:\\base\\a\\c.txt"
            } else {
                "/base/a/c.txt"
            })
        );
        // Out of the base entirely, which is the point of allowing it.
        let up = resolve(&base(), "../sibling.txt", "path").expect("resolved");
        assert_eq!(
            up,
            want(if cfg!(windows) {
                "C:\\sibling.txt"
            } else {
                "/sibling.txt"
            })
        );
    }

    #[test]
    fn dot_dot_above_the_root_stays_at_the_root() {
        let got = resolve(&base(), "../../../../x", "path").expect("resolved");
        assert_eq!(got, want(if cfg!(windows) { "C:\\x" } else { "/x" }));
    }

    #[test]
    fn surrounding_whitespace_is_trimmed_and_an_empty_path_is_refused() {
        assert_eq!(
            resolve(&base(), "  notes.txt\n", "path").expect("resolved"),
            want(if cfg!(windows) {
                "C:\\base\\notes.txt"
            } else {
                "/base/notes.txt"
            })
        );
        for empty in ["", "   ", "\n"] {
            let err = resolve(&base(), empty, "path").expect_err("refused");
            assert!(err.contains("is empty"), "{err}");
        }
    }

    /// **P090, the case that started it.** Windows Explorer's *Copy as path* (Ctrl+Shift+C)
    /// puts the quotes on the clipboard with the path, and a string beginning with `"` is
    /// not absolute -- so before the quotes came off, an absolute path was joined onto the
    /// base directory and the refusal read as a containment refusal that does not exist.
    #[test]
    fn a_path_pasted_with_the_quotes_explorer_puts_on_it_is_still_that_path() {
        let bare = if cfg!(windows) {
            r"F:\temp\test\flag.txt"
        } else {
            "/tmp/test/flag.txt"
        };
        // The literal bytes the clipboard carries: a quote, the path, a quote.
        let pasted = format!("\"{bare}\"");
        assert_eq!(
            resolve(&base(), &pasted, "path").expect("resolved"),
            want(bare),
            "the pasted form {pasted:?} must be the same file as {bare:?}"
        );
        // ...and with what a wire or a text box adds on either side of it. Whitespace can
        // be outside the quotes or inside them depending on which happened last, which is
        // why `resolve` trims, unquotes, and trims again.
        for wrapped in [
            format!("  {pasted}\n"),
            format!("\t{pasted} "),
            format!("\" {bare} \""),
            format!("'{bare}'"),
        ] {
            assert_eq!(
                resolve(&base(), &wrapped, "path").expect("resolved"),
                want(bare),
                "{wrapped:?}"
            );
        }
    }

    /// One pair, both ends, the same character -- and nothing else. Every other shape is
    /// a guess about which file was meant, and a guess is worse than a miss.
    #[test]
    fn only_a_matching_pair_of_surrounding_quotes_comes_off() {
        let joined = |name: &str| {
            let mut path = PathBuf::from(if cfg!(windows) { "C:\\base" } else { "/base" });
            path.push(name);
            path
        };
        // Single quotes come off too: a path pasted out of a POSIX shell carries those.
        assert_eq!(
            resolve(&base(), "'a.txt'", "path").expect("resolved"),
            joined("a.txt")
        );
        for untouched in [
            "\"a.txt'", // mismatched ends
            "'a.txt\"", // mismatched the other way
            "\"a.txt",  // one end only
            "a.txt\"",  // the other end only
            "a\"b.txt", // interior: an ordinary character in a name
            "\"",       // one character cannot be a pair
            "'",
        ] {
            assert_eq!(
                resolve(&base(), untouched, "path").expect("resolved"),
                joined(untouched),
                "{untouched:?} is a filename, not a quoted path"
            );
        }
        // Two pairs loses one, so a file genuinely named `"a.txt"` is still reachable by
        // quoting it again -- which is the whole answer to the trade the module doc states.
        assert_eq!(
            resolve(&base(), "\"\"a.txt\"\"", "path").expect("resolved"),
            joined("\"a.txt\"")
        );
        // A pair with nothing in it is empty, and empty is already a refusal.
        for empty in ["\"\"", "''", "  \"   \"  "] {
            let err = resolve(&base(), empty, "path").expect_err("refused");
            assert!(err.contains("is empty"), "{err}");
        }
    }

    /// The refusal a graph author reads when the path was right and the file was not.
    /// It has to say *how* the path was treated, because the same text is a different
    /// file depending on the answer, and counting backslashes in a joined path is not a
    /// reasonable thing to ask of someone.
    #[test]
    fn how_a_path_was_treated_is_said_in_words() {
        let absolute = if cfg!(windows) {
            r"F:\temp\thing.txt"
        } else {
            "/tmp/thing.txt"
        };
        let given = treatment(&base(), absolute, "path");
        assert!(given.contains("absolute"), "{given}");
        assert!(given.contains("exactly as given"), "{given}");

        let derived = treatment(&base(), "notes/a.txt", "path");
        assert!(derived.contains("relative"), "{derived}");
        // Naming what it was joined to is the whole point -- and where that came from,
        // because "the project's folder" and "the folder HollowDeck was launched from"
        // are different answers and only the core knows which one it sent.
        assert!(
            derived.contains("C:\\base") || derived.contains("/base"),
            "{derived}"
        );
        assert!(derived.contains("project"), "{derived}");

        // With no base there is nothing to name, and `resolve` has already refused; the
        // sentence still has to be a sentence rather than a dangling "joined to".
        let none = treatment(&Base::default(), "notes/a.txt", "path");
        assert!(none.contains("relative"), "{none}");
        assert!(!none.contains("joined to "), "{none}");
    }

    /// A relative path with no base is a refusal that says who was meant to supply one.
    #[test]
    fn a_relative_path_with_no_base_is_refused_and_says_why() {
        let err = resolve(&Base::default(), "notes.txt", "path").expect_err("refused");
        assert!(err.contains("relative"), "{err}");
        assert!(err.contains("broker") && err.contains("absolute"), "{err}");
        // An absolute one still works with no base at all.
        let absolute = if cfg!(windows) { "C:\\x.txt" } else { "/x.txt" };
        assert!(resolve(&Base::default(), absolute, "path").is_ok());
    }

    /// A base that is not absolute is no base. The core sends one it computed from the
    /// project file or from its own executable, and both are absolute; anything else is
    /// a caller making something up.
    #[test]
    fn a_relative_base_is_not_a_base() {
        let base = Base::of("relative/dir", "project");
        assert_eq!(base.dir, None);
        assert_eq!(base.source.as_deref(), Some("project"));
        assert!(resolve(&base, "notes.txt", "path").is_err());
    }

    #[test]
    fn a_trailing_separator_does_not_change_which_file_it_is() {
        let with = resolve(&base(), "notes/", "path").expect("resolved");
        let without = resolve(&base(), "notes", "path").expect("resolved");
        assert_eq!(with, without);
    }

    #[test]
    fn forward_and_back_slashes_mean_the_same_thing_on_windows() {
        let forward = resolve(&base(), "a/b/c.txt", "path").expect("resolved");
        let back = resolve(&base(), "a\\b\\c.txt", "path").expect("resolved");
        if cfg!(windows) {
            assert_eq!(forward, back);
        } else {
            // On POSIX a backslash is an ordinary character in a filename, and pretending
            // otherwise would make one path mean two things depending on the machine.
            assert_ne!(forward, back);
        }
    }

    #[test]
    fn a_drive_relative_path_is_refused_rather_than_guessed_at() {
        let err = resolve(&base(), "C:notes.txt", "path");
        if cfg!(windows) {
            let err = err.expect_err("refused");
            assert!(err.contains("names a drive but no directory"), "{err}");
        } else {
            // `C:notes.txt` is a perfectly ordinary relative filename on POSIX.
            assert!(err.is_ok());
        }
    }

    #[test]
    fn a_rooted_path_with_no_drive_keeps_the_bases_drive_on_windows() {
        let got = resolve(&base(), "\\notes\\a.txt", "path").expect("resolved");
        if cfg!(windows) {
            assert_eq!(got, want("C:\\notes\\a.txt"));
        }
    }

    #[test]
    fn a_unc_path_is_absolute_and_survives_untouched() {
        let unc = r"\\server\share\dir\file.txt";
        let got = resolve(&base(), unc, "path").expect("resolved");
        if cfg!(windows) {
            assert_eq!(got, want(unc));
        }
    }

    /// `\\?\` means *do not normalise me*. Honour that rather than tidying `..` out of it.
    #[test]
    fn a_verbatim_path_is_left_exactly_as_it_was_given() {
        let verbatim = r"\\?\C:\a\..\b.txt";
        let got = resolve(&base(), verbatim, "path").expect("resolved");
        if cfg!(windows) {
            assert_eq!(got, want(verbatim));
        }
    }

    /// A drive letter and nothing else is a directory, and resolving says so without
    /// deciding whether anything may be read from it.
    #[test]
    fn a_bare_root_resolves_to_the_root() {
        let root = if cfg!(windows) { "C:\\" } else { "/" };
        assert_eq!(
            resolve(&base(), root, "path").expect("resolved"),
            want(root)
        );
    }

    /// Reserved device names are **not** refused. P049 refused them by name because a
    /// write to `<dir>\NUL` "succeeded" while creating nothing *inside a jail that was
    /// supposed to contain it*. There is no jail now, and `NUL` is a real thing to write
    /// to on purpose. What the tool reports for one is measured, not assumed --
    /// `tools::tests`.
    #[test]
    fn a_device_name_is_a_path_like_any_other_here() {
        assert!(resolve(&base(), "NUL", "path").is_ok());
        assert!(resolve(&base(), "CON", "path").is_ok());
    }
}

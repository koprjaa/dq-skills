//
// Project: dq-skills
// File:    skills.rs
//
// Description:
// Checks that every skill file carries the frontmatter and the pipeline wiring Claude Code expects.
//
// Author:
// Jan Alexandr Kopřiva
// jan.alexandr.kopriva@gmail.com
//
// License: MIT
//

//! A skill with broken frontmatter is not loaded and not reported, so nothing
//! tells you it is missing until the pipeline silently skips a step.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const ENTRY: &str = "dq-pipeline";

// The order the pipeline runs in. The entry point sits outside it, holding the map and the shared standard.
const PIPELINE: &[&str] = &[
    "dq-profiler",
    "dq-validator",
    "dq-auditor",
    "dq-parser",
    "dq-standardizer",
    "dq-address",
    "dq-imputer",
    "dq-deduplicator",
    "dq-guard",
];

struct Skill {
    name: String,
    text: String,
}

fn skills_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("skills")
}

fn skills() -> Vec<Skill> {
    let mut found: Vec<Skill> = fs::read_dir(skills_dir())
        .expect("skills directory")
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let text = fs::read_to_string(e.path().join("SKILL.md")).ok()?;
            // Git on Windows may check out CRLF.
            Some(Skill {
                name: e.file_name().to_string_lossy().into_owned(),
                text: text.replace("\r\n", "\n"),
            })
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// Returns the top-level frontmatter fields and the body, or None when there is no frontmatter.
fn split(text: &str) -> Option<(BTreeMap<String, String>, &str)> {
    let rest = text.strip_prefix("---\n")?;
    let end = rest.find("\n---\n")?;
    let fields = rest[..end]
        .lines()
        .filter(|l| !l.starts_with([' ', '\t']))
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect();
    Some((fields, &rest[end + 5..]))
}

fn field(skill: &Skill, key: &str) -> String {
    split(&skill.text)
        .and_then(|(f, _)| f.get(key).cloned())
        .unwrap_or_default()
}

/// Fails once with every offending skill named, instead of stopping at the first.
fn assert_all(what: &str, bad: Vec<String>) {
    assert!(bad.is_empty(), "{what}: {}", bad.join(", "));
}

#[test]
fn every_skill_directory_holds_a_skill_file() {
    let dirs: BTreeSet<String> = fs::read_dir(skills_dir())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let with_file: BTreeSet<String> = skills().into_iter().map(|s| s.name).collect();
    assert_eq!(dirs, with_file);
}

#[test]
fn a_skill_opens_with_frontmatter() {
    assert_all(
        "no readable frontmatter",
        skills()
            .into_iter()
            .filter(|s| split(&s.text).is_none())
            .map(|s| s.name)
            .collect(),
    );
}

/// Fields outside the Agent Skills spec are ignored, so a misplaced one fails silently.
#[test]
fn the_frontmatter_uses_only_known_fields() {
    const ALLOWED: &[&str] = &[
        "name",
        "description",
        "allowed-tools",
        "compatibility",
        "license",
        "metadata",
        "user-invocable",
    ];
    let bad = skills()
        .iter()
        .filter_map(|s| {
            let (fields, _) = split(&s.text)?;
            let extra: Vec<&str> = fields
                .keys()
                .map(String::as_str)
                .filter(|k| !ALLOWED.contains(k))
                .collect();
            (!extra.is_empty()).then(|| format!("{} ({})", s.name, extra.join(", ")))
        })
        .collect();
    assert_all("unknown frontmatter field", bad);
}

/// allowed-tools pre-approves tools without a permission prompt, it does not restrict them.
#[test]
fn no_skill_pre_approves_tools() {
    assert_all(
        "allowed-tools set",
        skills()
            .into_iter()
            .filter(|s| !field(s, "allowed-tools").is_empty())
            .map(|s| s.name)
            .collect(),
    );
}

/// A step without its mistakes table hides the rules it is easiest to break.
#[test]
fn every_step_lists_common_mistakes() {
    let bad = skills()
        .into_iter()
        .filter(|s| PIPELINE.contains(&s.name.as_str()))
        .filter(|s| !s.text.contains("\n## Common mistakes"))
        .map(|s| s.name)
        .collect();
    assert_all("no ## Common mistakes section", bad);
}

/// Claude Code resolves a skill by directory, so a mismatch is unreachable.
#[test]
fn the_name_matches_the_directory() {
    assert_all(
        "name differs from directory",
        skills()
            .into_iter()
            .filter(|s| field(s, "name") != s.name)
            .map(|s| s.name)
            .collect(),
    );
}

/// The description is what decides whether a skill activates.
#[test]
fn the_description_says_enough_to_be_matched_on() {
    let bad = skills()
        .iter()
        .filter_map(|s| {
            let n = field(s, "description").chars().count();
            (n < 80).then(|| format!("{} ({n} characters)", s.name))
        })
        .collect();
    assert_all("description too short", bad);
}

/// Keywords catch the terms a user types when the prose of the description does not.
#[test]
fn the_description_lists_keywords() {
    assert_all(
        "no Keywords: in description",
        skills()
            .into_iter()
            .filter(|s| !field(s, "description").contains("Keywords:"))
            .map(|s| s.name)
            .collect(),
    );
}

#[test]
fn a_skill_has_a_body() {
    let bad = skills()
        .into_iter()
        .filter(|s| split(&s.text).is_none_or(|(_, body)| body.trim().chars().count() <= 200))
        .map(|s| s.name)
        .collect();
    assert_all("body too short", bad);
}

#[test]
fn every_pipeline_step_exists() {
    let present: BTreeSet<String> = skills().into_iter().map(|s| s.name).collect();
    assert_all(
        "missing step",
        PIPELINE
            .iter()
            .filter(|p| !present.contains(**p))
            .map(|p| (*p).to_owned())
            .collect(),
    );
}

#[test]
fn the_entry_point_exists() {
    assert!(skills_dir().join(ENTRY).join("SKILL.md").exists());
}

/// The order is load-bearing: deduplicating before standardizing misses matches.
#[test]
fn the_pipeline_order_is_documented_in_the_entry_point() {
    let text = fs::read_to_string(skills_dir().join(ENTRY).join("SKILL.md")).unwrap();
    assert_all(
        "not named in the entry point",
        PIPELINE
            .iter()
            .filter(|p| !text.contains(**p))
            .map(|p| (*p).to_owned())
            .collect(),
    );
}

/// Each step has to say what runs before and after it, or the order is lost.
/// Naming the entry point or another step is the proof; a bare "dq-" prefix is not.
#[test]
fn a_skill_names_its_place_in_the_pipeline() {
    let bad = skills()
        .into_iter()
        .filter(|s| s.name != ENTRY)
        .filter(|s| {
            let t = s.text.to_lowercase();
            !t.contains(ENTRY) && !PIPELINE.iter().any(|p| *p != s.name && t.contains(p))
        })
        .map(|s| s.name)
        .collect();
    assert_all("no reference to another step or the entry point", bad);
}

/// A skill directory missing from PIPELINE would escape every check above.
#[test]
fn every_skill_is_a_pipeline_step_or_the_entry_point() {
    let bad = skills()
        .into_iter()
        .filter(|s| s.name != ENTRY && !PIPELINE.contains(&s.name.as_str()))
        .map(|s| s.name)
        .collect();
    assert_all("not listed in PIPELINE", bad);
}

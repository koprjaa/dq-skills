#
# Project: dq-skills
# File:    test_skills.py
#
# Description:
# Checks that every skill file carries the frontmatter and the pipeline wiring Claude Code expects.
#
# Author:
# Jan Alexandr Kopřiva
# jan.alexandr.kopriva@gmail.com
#
# License: MIT
#

"""Checks every skill file is shaped the way Claude Code expects.

A skill with broken frontmatter is not loaded and not reported, so nothing
tells you it is missing until the pipeline silently skips a step.
"""

import re
from pathlib import Path

import pytest

SKILLS_DIR = Path(__file__).resolve().parent.parent / "skills"
FRONTMATTER_RE = re.compile(r"\A---\n(.*?)\n---\n", re.S)

# The order the pipeline runs in. dq-pipeline is the entry point and sits
# outside it, holding the map and the shared standard.
PIPELINE = [
    "dq-profiler",
    "dq-validator",
    "dq-auditor",
    "dq-parser",
    "dq-standardizator",
    "dq-adresar",
    "dq-imputator",
    "dq-deduplikator",
    "dq-strazce",
]

SKILL_FILES = sorted(SKILLS_DIR.glob("*/SKILL.md"))


def frontmatter(path: Path) -> dict:
    match = FRONTMATTER_RE.match(path.read_text(encoding="utf-8"))
    if not match:
        return {}
    fields = {}
    for line in match.group(1).split("\n"):
        if ":" in line and not line.startswith((" ", "\t")):
            key, _, value = line.partition(":")
            fields[key.strip()] = value.strip()
    return fields


def test_every_skill_directory_holds_a_skill_file():
    directories = {d.name for d in SKILLS_DIR.iterdir() if d.is_dir()}
    with_file = {p.parent.name for p in SKILL_FILES}
    assert directories == with_file


@pytest.mark.parametrize("path", SKILL_FILES, ids=lambda p: p.parent.name)
def test_a_skill_opens_with_frontmatter(path):
    assert frontmatter(path), f"{path.parent.name} has no readable frontmatter"


@pytest.mark.parametrize("path", SKILL_FILES, ids=lambda p: p.parent.name)
def test_the_name_matches_the_directory(path):
    """Claude Code resolves a skill by directory, so a mismatch is unreachable."""
    assert frontmatter(path).get("name") == path.parent.name


@pytest.mark.parametrize("path", SKILL_FILES, ids=lambda p: p.parent.name)
def test_the_description_says_enough_to_be_matched_on(path):
    """The description is what decides whether a skill activates."""
    description = frontmatter(path).get("description", "")
    assert len(description) >= 80, f"{path.parent.name}: {len(description)} characters"


@pytest.mark.parametrize("path", SKILL_FILES, ids=lambda p: p.parent.name)
def test_a_skill_has_a_body(path):
    body = FRONTMATTER_RE.sub("", path.read_text(encoding="utf-8"))
    assert len(body.strip()) > 200


def test_every_pipeline_step_exists():
    present = {p.parent.name for p in SKILL_FILES}
    assert set(PIPELINE) <= present


def test_the_entry_point_exists():
    assert (SKILLS_DIR / "dq-pipeline" / "SKILL.md").exists()


def test_the_pipeline_order_is_documented_in_the_entry_point():
    """The order is load-bearing: deduplicating before standardizing misses matches."""
    text = (SKILLS_DIR / "dq-pipeline" / "SKILL.md").read_text(encoding="utf-8")
    positions = [text.find(name) for name in PIPELINE]
    assert all(p >= 0 for p in positions), "a step is not named in dq-pipeline"


@pytest.mark.parametrize("path", SKILL_FILES, ids=lambda p: p.parent.name)
def test_a_skill_names_its_place_in_the_pipeline(path):
    """Each step has to say what runs before and after it, or the order is lost."""
    text = path.read_text(encoding="utf-8").lower()
    assert "pipeline" in text or "dq-" in text

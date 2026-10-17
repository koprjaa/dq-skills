# dq-skills

Ten Claude Code skills that audit and clean data quality in any SQL database, from profiling and quality metrics to the audit report, standardization, imputation, deduplication and defect prevention.

![claude-code](https://img.shields.io/badge/Claude%20Code-skills-D97757?style=flat-square)
![license](https://img.shields.io/badge/license-MIT-A31F34?style=flat-square)
![status](https://img.shields.io/badge/status-active-22863A?style=flat-square)
[![ci](https://github.com/koprjaa/dq-skills/actions/workflows/ci.yml/badge.svg)](https://github.com/koprjaa/dq-skills/actions/workflows/ci.yml)

The method comes from the course 4IZ562 Data Quality Management at Prague University of Economics and Business, and from an audit of 23 insurance company tables with 64 findings, a cost of poor quality estimate, and the corrective actions that followed. Nothing in the skills is specific to insurance, and the same defect patterns turn up in an online shop or a public registry.

## Pipeline

```
dq-profiler > dq-validator > dq-auditor                     audit
     |
dq-parser > dq-standardizer > dq-address > dq-imputer > dq-deduplicator > dq-guard
                                                            remediation
```

| Skill | Function |
|---|---|
| `dq-pipeline` | Entry point. Quality dimensions, finding format, how to port the SQL and the domain rules. |
| `dq-profiler` | Inventory, structure, collation, primary and foreign keys, distributions, defect pattern catalogue. |
| `dq-validator` | Checks across six dimensions, checksum validators, writes the score to a metadata repository. |
| `dq-auditor` | Finding catalogue, root cause, cost of poor quality and return on investment, legal context, priority. |
| `dq-parser` | Splits composite values, such as house number out of street, municipality against district, person name. |
| `dq-standardizer` | Canonical form, link to reference dictionaries, display value against match key. |
| `dq-address` | Link to the address registry, match code, accuracy hierarchy, match rate. |
| `dq-imputer` | Fills missing values by source reliability, and states when not to impute. |
| `dq-deduplicator` | Match code, clusters, survivor selection, golden record, household. |
| `dq-guard` | Data types, foreign key and check constraints, quality firewall, monitoring, data governance. |

Run the steps in this order. Deduplicating before standardization leaves `MuDr` and `MUDr.` as two people, and constraints added before remediation fail on the existing data.

## Install

```bash
git clone https://github.com/koprjaa/dq-skills.git
cp -R dq-skills/skills/* ~/.claude/skills/
```

For one project only, copy into `.claude/skills/` inside that project.

## Use

The skills activate from their description. Write what you want:

```
profile this database for me
measure the data quality of the customer table
write the audit report from what you found
clean and deduplicate the client base
```

You can also call one directly with `/dq-profiler` or `/dq-validator`.

Start with `dq-pipeline` when you do not know which step comes first. It holds the pipeline map and the shared standard.

## Databases

The SQL in the catalogues targets MySQL. `dq-pipeline` holds a translation table for PostgreSQL, SQL Server, Oracle, SQLite, and DuckDB. The table covers regular expressions, metadata, hashes, conditional aggregates, and collation.

The domain validators for birth number, company identifier, postal code, address registry, and industry classification are Czech examples. The structure of each check stays the same. Replace the specific rule with the equivalent for your jurisdiction, using the mapping table in `dq-pipeline`.

## Principles

- No claim about quality without a query and a number.
- Every number needs its universe. "5,328 missing" alone means nothing.
- Never overwrite the original column. Remediation writes to `_STD` columns.
- A suspicion becomes a finding once a second, independent query confirms it.
- Fixing the data without fixing the input control means the defect returns.

## Limits

The skills contain documentation and SQL. They ship no runnable code. Example values such as names, addresses and registries are Czech because the source audit was Czech.

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The tests check the shape of each skill file. The frontmatter has to parse, the name has to match the directory, and the description has to be long enough to match on and end with a keyword list. Every pipeline step has to appear in `dq-pipeline`. Claude Code skips a skill with broken frontmatter without an error, so without these tests a missing step shows up only when the pipeline runs past it.

## Related

[da-skills](https://github.com/koprjaa/da-skills) designs or reviews the data architecture these checks run against. Its steps hand off to `dq-profiler`, `dq-deduplicator`, and `dq-guard`.

## License

[MIT](LICENSE)

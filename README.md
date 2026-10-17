# dq-skills

Claude Code skills for auditing and cleaning data quality in SQL databases.

![claude-code](https://img.shields.io/badge/Claude%20Code-skills-D97757?style=flat-square)
![license](https://img.shields.io/badge/license-MIT-A31F34?style=flat-square)
[![ci](https://github.com/koprjaa/dq-skills/actions/workflows/ci.yml/badge.svg)](https://github.com/koprjaa/dq-skills/actions/workflows/ci.yml)

Ten skills: profiling, quality metrics, the audit report, then parsing, standardization, address matching, imputation, deduplication and prevention. The method comes from the course 4IZ562 Data Quality Management at Prague University of Economics and Business and from an audit of 23 tables of an insurance company (64 findings).

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

Keep the order. Deduplicating before standardization treats `MuDr` and `MUDr.` as different values, and constraints added before remediation fail on existing data.

## Install

```bash
git clone https://github.com/koprjaa/dq-skills.git
cp -R dq-skills/skills/* ~/.claude/skills/
```

For one project only, copy into `.claude/skills/` inside that project.

## Use

Plain requests work:

```
profile this database for me
measure the data quality of the customer table
write the audit report from what you found
clean and deduplicate the client base
```

Or call one directly, for example `/dq-profiler`. If you are not sure where to start, use `/dq-pipeline`.

## Databases

The SQL is written for MySQL. `dq-pipeline` has a translation table for PostgreSQL, SQL Server, Oracle, SQLite and DuckDB.

Validators for birth numbers, company IDs, postal codes, addresses and industry codes use Czech rules. `dq-pipeline` lists what to replace them with elsewhere.

## Principles

- Every finding has a query and a number.
- Every number has its base: "5,328 of 119,348 missing", not "5,328 missing".
- Remediation writes to `_STD` columns and never overwrites the original.
- Fix the input control as well as the data, or the defect comes back.

## Limits

The skills are documentation and SQL, with no runnable code. Example names, addresses and registries are Czech.

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The tests check that every skill file has valid frontmatter, a name matching its directory and a description with keywords, and that every step is listed in `dq-pipeline`. Claude Code silently skips a skill with broken frontmatter.

## Related

[da-skills](https://github.com/koprjaa/da-skills) designs or reviews the data architecture and calls `dq-profiler`, `dq-deduplicator` and `dq-guard` at the matching steps.

## License

[MIT](LICENSE)

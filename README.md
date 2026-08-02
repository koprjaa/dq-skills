# dq-skills

Ten Claude Code skills that audit and clean data quality in any SQL database. They cover profiling an unknown database, measuring quality metrics, writing the audit report, then standardizing, imputing, deduplicating, and preventing repeat defects.

![claude-code](https://img.shields.io/badge/Claude%20Code-skills-D97757?style=flat-square)
![license](https://img.shields.io/badge/license-MIT-A31F34?style=flat-square)
![status](https://img.shields.io/badge/status-active-22863A?style=flat-square)
[![ci](https://github.com/koprjaa/dq-skills/actions/workflows/ci.yml/badge.svg)](https://github.com/koprjaa/dq-skills/actions/workflows/ci.yml)

The method comes from the course 4IZ562 Data Quality Management at Prague University of Economics and Business, and from an audit of 23 insurance company tables with 64 findings, a cost of poor quality estimate, and the corrective actions that followed. The skills are written without a domain. The defect patterns apply the same way to an online shop, a hospital, or a state registry.

## Pipeline

```
dq-profiler > dq-validator > dq-auditor                     audit
     |
dq-parser > dq-standardizator > dq-adresar > dq-imputator > dq-deduplikator > dq-strazce
                                                            remediation
```

| Skill | Function |
|---|---|
| `dq-pipeline` | Entry point. Quality dimensions, finding format, how to port the SQL and the domain rules. |
| `dq-profiler` | Inventory, structure, collation, primary and foreign keys, distributions, defect pattern catalogue. |
| `dq-validator` | Checks across six dimensions, checksum validators, writes the score to a metadata repository. |
| `dq-auditor` | Finding catalogue, root cause, cost of poor quality and return on investment, legal context, priority. |
| `dq-parser` | Splits composite values, such as house number out of street, municipality against district, person name. |
| `dq-standardizator` | Canonical form, link to reference dictionaries, display value against match key. |
| `dq-adresar` | Link to the address registry, match code, accuracy hierarchy, match rate. |
| `dq-imputator` | Fills missing values by source reliability, and states when not to impute. |
| `dq-deduplikator` | Match code, clusters, survivor selection, golden record, household. |
| `dq-strazce` | Data types, foreign key and check constraints, quality firewall, monitoring, data governance. |

The order matters. Deduplication before standardization does not merge `MuDr` with `MUDr.`. Constraints before remediation fail.

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
- A cross check turns a suspicion into evidence.
- Fixing the data without fixing the input control means the defect returns.

## Limits

The skills contain documentation and SQL. They ship no runnable code, and the skill files themselves are written in Czech.

## Development

```bash
uv run --extra dev ruff check .
uv run --extra dev pytest -q
```

The suite checks the shape of each skill file rather than its content: that the
frontmatter parses, that the name matches its directory, that the description is
long enough to be matched on, and that every pipeline step is named in
`dq-pipeline`. A skill with broken frontmatter is not loaded and not reported,
so nothing tells you a step is missing until the pipeline silently skips it.

## License

[MIT](LICENSE)

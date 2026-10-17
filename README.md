# dq-skills

Claude Code skills for auditing and cleaning data quality in SQL databases.

[![ci](https://github.com/koprjaa/dq-skills/actions/workflows/ci.yml/badge.svg)](https://github.com/koprjaa/dq-skills/actions/workflows/ci.yml)
![license](https://img.shields.io/badge/license-MIT-A31F34?style=flat-square)

Based on the course 4IZ562 Data Quality Management at Prague University of Economics and Business.

## Skills

Run them in this order.

| Skill | What it does |
|---|---|
| `dq-pipeline` | Start here. Explains the steps and the shared formats. |
| `dq-profiler` | Maps an unknown database: tables, keys, value distributions, suspicious patterns. |
| `dq-validator` | Measures quality: completeness, validity, consistency, duplicates. |
| `dq-auditor` | Writes the audit report: findings, causes, cost, priorities. |
| `dq-parser` | Splits values stored together, such as street and house number. |
| `dq-standardizer` | Unifies how values are written. |
| `dq-address` | Matches addresses to the official address registry. |
| `dq-imputer` | Fills in missing values where they can be derived. |
| `dq-deduplicator` | Finds duplicate records and merges them. |
| `dq-guard` | Stops the errors from coming back: constraints, input checks, monitoring. |

## Install

```bash
git clone https://github.com/koprjaa/dq-skills.git
cp -R dq-skills/skills/* ~/.claude/skills/
```

## Use

Ask in plain words, for example "profile this database" or "deduplicate the customer table", or call a skill directly with `/dq-profiler`.

The SQL is written for MySQL, and `dq-pipeline` shows how to translate it for PostgreSQL, SQL Server, Oracle, SQLite and DuckDB. Identifier and address checks use Czech rules.

See also [da-skills](https://github.com/koprjaa/da-skills) for designing the data architecture.

## License

[MIT](LICENSE)

---
name: dq-profiler
description: Technical profiling of a database. Table inventory, structure and data types, collation, PK/FK/indexes, frequency distributions, samples, and detection of defect patterns (blanket constants, sentinel values, padding, truncated values, orphan ID ranges). The first step of the DQ pipeline, before dq-validator. Use it when someone wants to "look at the database", "find out what is in this data", "profiling", "data exploration", "what is wrong with this database". Keywords: profiling, data profiling, inventory, table structure, collation, value distribution, database exploration.
---

# Profiler: technical profiling

First step. **Look for the shape of the data, not yet for error counts.** Do notice symptoms (outliers,
anomalies, suspicious constants belong here), but systematic measurement against rules is done by
`dq-validator` once you know what the tables contain. The profiler ends with a list of tables, their roles
and a list of suspicions for the validator to verify.

Pipeline: **dq-profiler** → `dq-validator` → `dq-auditor` → remediation. The shared standard (dimensions,
finding format) is in `dq-pipeline`.

## Step 1: inventory and table roles

Sort every table into three bins. Without that you do not know what the audit universe is and what the
reference standard is that measures it:

| Bin | How to recognise it | Role in the audit |
|---|---|---|
| **operational** | transactional/master data, grows over time | the audit universe |
| **code list (LOV)** | a few dozen rows, `CODE`+`VALUE`, internal domain | yardstick for SEM_CORR |
| **reference (REF)** | external standard or registry, hundreds of thousands to millions of rows | yardstick for EXT_CONS |

Do not trust the prefix in the name, verify by content. In the audited database `REF_TITBEF`, `REF_FNAME`
and `REF_NACE` were plain code lists (LOV) by structure, while `LOV_COUNTRY` and `LOV_ESA95` were external
standards (ISO 3166-1, ESA95) → should have been REF. Mixed-up prefixes are a finding in themselves.

```sql
SELECT TABLE_NAME, TABLE_ROWS, ENGINE, TABLE_COLLATION
FROM information_schema.TABLES WHERE TABLE_SCHEMA = DATABASE() ORDER BY TABLE_ROWS DESC;
```

`TABLE_ROWS` is an estimate in InnoDB. For metrics always use `COUNT(*)`.

## Step 2: structure, types, collation (schema consistency)

```sql
SELECT TABLE_NAME, COLUMN_NAME, DATA_TYPE, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT,
       CHARACTER_SET_NAME, COLLATION_NAME
FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE()
ORDER BY TABLE_NAME, ORDINAL_POSITION;
```

What to read from the output:

| Signal | What it means | Real example |
|---|---|---|
| `char(n)` on variable-length text | space padding + silent truncation of longer input | `PARTY_TITBEF char(10)` → `'PhDr      '`; `ADDR_ZIP char(5)` + input `251 62` → stored `251 6` |
| `char(4)` / `tinyint` / `char(1)` for the same Y/N flag | three implementations of one concept | `DEL_FLAG` across 9 code lists |
| nullable on a logically mandatory column | missing constraint, the application does not enforce it | `PARTY_TYPE` nullable |
| `NOT NULL` + empty default | "filled" means an empty string | `ADDR_TYPE NOT NULL DEFAULT ''` |
| several collations in one table | JOIN fails with Error 1267 or silently does not match | `ADDR_CITY utf8mb3_czech_ci` vs `ADDR_TYPE utf8mb4_0900_ai_ci` |
| numeric type on a code with a leading zero | zeros lost (company IDs, postcodes) | `ADDR_ZIP_STD integer` |
| localised abbreviations in an otherwise English schema | naming convention broken selectively | `PARTY_RC`, `PARTY_OKEC` |

Map collation **as a matrix**: for every planned JOIN compare the collation of both sides. Record a
mismatch as a finding right away; do not carry it into the validator as a surprise.

```sql
SELECT COLLATION_NAME, COUNT(*) columns, GROUP_CONCAT(DISTINCT TABLE_NAME) tables
FROM information_schema.COLUMNS
WHERE TABLE_SCHEMA = DATABASE() AND COLLATION_NAME IS NOT NULL GROUP BY 1;
```

## Step 3: keys, indexes, referential integrity

```sql
-- PKs and unique indexes
SELECT TABLE_NAME, INDEX_NAME, NON_UNIQUE,
       GROUP_CONCAT(COLUMN_NAME ORDER BY SEQ_IN_INDEX) cols
FROM information_schema.STATISTICS WHERE TABLE_SCHEMA = DATABASE()
GROUP BY 1,2,3 ORDER BY 1;

-- declared FKs
SELECT TABLE_NAME, COLUMN_NAME, REFERENCED_TABLE_NAME, REFERENCED_COLUMN_NAME
FROM information_schema.KEY_COLUMN_USAGE
WHERE TABLE_SCHEMA = DATABASE() AND REFERENCED_TABLE_NAME IS NOT NULL;
```

**An empty result of the second query is a critical finding, not a technicality.** In the audited
database not one of 9 code lists and 8 reference tables had an FK; the only source of integrity was the
application layer, which demonstrably failed (91,693 contracts with a frequency code outside the code list,
382,210 addresses with a type outside the code list, 37,879 orphaned contacts).

Build a **list of missing FKs**: every relationship the data model implies but the schema does not enforce.
That list is an appendix of the report and at the same time the brief for `dq-guard`.

Watch PKs too: a surrogate `auto_increment` PK does not yet mean business uniqueness. `PARTY_ADDRESS` had a
PK on `ADDR_ID`, but nothing prevented storing the same address for a client three times. A one-character
PK (`CODE char(1)`) is fragile; extending the code list leads to a collision.

## Step 4: volume and basic characteristics

For every operational table: `COUNT(*)`, a breakdown by entity type, min/max on dates and numbers, distinct
counts on category candidates.

For numeric columns do not stop at the mean; a single extreme drags it anywhere. Quantiles, variance and
standard deviation, skewness and kurtosis say much more about the distribution; a trimmed mean is a cheap
safeguard against outliers.

```sql
SELECT PARTY_TYPE, COUNT(*) freq FROM PART_PARTY GROUP BY 1 ORDER BY freq DESC;
SELECT MIN(CNTR_VALIDFROM), MAX(CNTR_VALIDFROM), MIN(CNTR_VALIDTO), MAX(CNTR_VALIDTO)
FROM PROD_CONTRACT;
```

`MAX` on a date is one of the cheapest sentinel detectors: when it returns `2999-12-31` or `3000-01-01`,
you have both the finding and the explanation.

## Step 5: frequency distributions (the core of profiling)

For every categorical and suspicious column:

```sql
SELECT <col>, COUNT(*) freq, ROUND(100.0*COUNT(*)/(SELECT COUNT(*) FROM <t>),2) pct
FROM <t> GROUP BY 1 ORDER BY freq DESC LIMIT 20;

-- the length distribution reveals padding and truncation
SELECT LENGTH(<col>) len, COUNT(*) freq FROM <t> GROUP BY 1 ORDER BY 1;

-- the day-of-month distribution reveals a day/month swap at import
SELECT DAY(<date_col>) day, COUNT(*) FROM <t> GROUP BY 1 ORDER BY 1;

-- the year distribution reveals a batch import
SELECT YEAR(<date_col>) year, COUNT(*) FROM <t> GROUP BY 1 ORDER BY 2 DESC LIMIT 15;
```

An unnatural peak in one year = a migration or a batch import. In the audited database 26,000+ clients had
birth year 2010 and **all** 31,764 back-dated contracts came from 2012.

## Step 6: look at a sample with your eyes, not with a query

`SELECT * FROM <t> LIMIT 5` and look properly. The first five rows of `PARTY_ADDRESS` showed at once: empty
`ADDR_NUM1/NUM2`, the house number merged into `ADDR_STREET` ("Na Budíně 854"), a constant `CZE` and a
duplicate address for two different clients. Four findings from one look.

## Outliers

`MIN`/`MAX` is a phantom detector, not an outlier analysis. Two things to add:

- **An outlier need not be an error.** A sum insured an order of magnitude above the rest is either a typo
  or the largest client in the portfolio. Domain judgement decides, not a quantile; before you call it a
  defect, ask someone who understands the process.
- **There is an apparatus for deciding "noise or signal".** The 4-sigma rule and a box plot for quick
  screening, Grubbs' or Dixon's test for one suspicious value, Chauvenet's and Peirce's criteria, and for
  multivariate data density methods (LOF, LOCI, INFLO). The distribution of the data chooses the method, not
  habit; with a non-normal distribution the sigma rule lies.

The report states which method identified the outliers and how many there were. Not just the final count
of "errors".

## Defect pattern catalogue

Look for these in the distributions and samples. Each row is a hypothesis for `dq-validator`.

| Pattern | What it looks like | Root cause | Verification |
|---|---|---|---|
| **Blanket constant** (*phantom*) | one value in 100 % of rows | column filled by a default, not from input | cross-check with an attribute that contradicts the constant |
| **Placeholder instead of NULL** | `NA`, `NEVYPLNENO`, `cizinec`, `9999999999`, `.` | the input field is NOT NULL, the operator must type something | frequency distribution, TOP values |
| **Sentinel date** | `2999-12-31`, `2999-01-01`, `1900-01-01`, `3000-01-01` | the system cannot store NULL / "unlimited validity" | `MAX()`/`MIN()` on dates |
| **Fixed-length padding** | `'PhDr      '`, length exactly `n` | import into `char(n)` without trim | length distribution |
| **Truncated value** | `251 6`, `400 0` instead of `251 62` | `char(5)` + input with a space | regex validation + sample |
| **Interleaved spaces** | `D A V I D` | manual retyping or a broken parser | `REGEXP '^([A-Za-z] )+'` |
| **Encoding error** | `ĄUDOVÍT` (should be `ĽUDOVÍT`), `BE0ŠOVÁ` | wrong character set at migration | look for digits in names and characters foreign to the language |
| **Systematic character swap** | `gmail#com`, `gmail&cz` | character mapping at import | count occurrences of the suspicious character, not just "invalid" |
| **Orphans in a contiguous ID range** | `range_size = distinct_ids`, start = `max(parent)+1` | an unfinished batch import from another system | `MIN`/`MAX`/`COUNT(DISTINCT)` on orphaned IDs |
| **Shadow taxonomy** | codes outside the code list, but used en masse | sales staff introduced their own values | anti-join to the code list + frequency |
| **Single-table pattern** | two entity types in one table without subtyping | missing subtyping | completeness broken down by type |
| **Back-dating** | `valid_from > valid_to`, all from one year | batch import / late channel integration | year distribution of the bad rows |
| **Day/month swap** | days 13–31 suspiciously rare | MM/DD vs DD/MM locale at import | day-of-month distribution |
| **Redundant column** | `VALUE` == `DESCR` in 100 % of rows | copy at design time, one stopped being maintained | `SUM(VALUE = DESCR)` |
| **Duplicate code for the same value** | `Dipl.tech.` under CODE 19 and 20 | manual code list maintenance | `GROUP BY TRIM(VALUE) HAVING COUNT(*)>1` |
| **Dead code list entry** | code exists, 0 uses | retired product / reserve / typo | LEFT JOIN from the code list to operations |
| **Stub reference table** | 1,000 rows where a registry should be | faulty import or a limited sample from the vendor | coverage = how many operational values are in REF |

Placeholder and sentinel values filled in across the board by the system are covered by one term:
**phantom value**.

## Output of the profiler

One document (or notebook) per table, with a uniform structure:

```
# <TABLE>
Name (what the abbreviation means) · About the table · Role in the data model · Dependencies
## 2. Structure and content
  2.1 Column overview (type, nullable, collation) + what follows from it
  2.2 Basic characteristics (counts, breakdown by entity type)
  2.3 Data sample + what it shows
## Suspicions for validation
  list of hypotheses with a reference to a pattern in the catalogue
```

Do not skip the "Role in the data model" section; without it the later steps cannot tell which code lists
and registries are the yardstick for the table.

These documents are **working material, not the deliverable**. The deliverable is one complete auditor's
report (`dq-auditor`) following ISACA standards; the per-table documents are aggregated into it and belong
in full in the appendices.

## Tools

SQL and the system catalogue are enough for everything above. With Python at hand, `ydata-profiling`
produces a basic table profile with one command (distributions, missing values, correlations); treat it as
a first pass, not as the output; it will not interpret the data or choose the universe for you. Named
tools in the field: Ataccama DQ Analyzer, DataCleaner, OpenRefine, and for continuous validation in a
pipeline, among open source tools, Great Expectations, Soda Core, Deequ or dbt tests.

## Common mistakes

| Mistake | Consequence | Instead |
|---|---|---|
| Sorting tables by their `REF_`/`LOV_` prefix | a code list is used as a registry yardstick, or the other way round | classify each table by its content (Step 1) |
| Taking `TABLE_ROWS` as the row count | an InnoDB estimate ends up in the metrics | `COUNT(*)` for every number that is reported |
| Treating an empty FK list as a technicality | integrity rests on an application that demonstrably fails | record it as a critical finding and hand the missing-FK list to `dq-guard` |
| Reading a numeric column by its mean | one extreme drags the picture anywhere | quantiles, spread and a trimmed mean (Step 4) |
| Calling every outlier an error | the largest client in the portfolio is reported as a typo | domain judgement first, then a method chosen by the distribution |
| Skipping the look at sample rows | findings a single glance would show stay hidden | `SELECT * … LIMIT 5` per table (Step 6) |

---
name: dq-validator
description: Data validation and measurement of quality metrics in any database. Checks across six dimensions (completeness, syntactic and semantic correctness, internal and external consistency, uniqueness) over operational tables, code lists and reference registries, checksum validators for identifiers (national ID, company ID, VAT, SSN), regex rules for e-mail, phone and postal code, and writing scores into a metadata repository. Second step of the DQ pipeline, after dq-profiler, before dq-auditor. Use it whenever someone wants to "run checks on the data", "measure data quality", "validate attributes", "compute a DQ score", "find invalid or inconsistent records", "verify referential integrity", even if the user never says validation. Keywords: validation, data validation, checks, DQ metrics, regex validation, checksum, mod 11, referential integrity, orphans, DQM_MDR, completeness, consistency, duplicates.
---

# Validator: checks and measurement

The profiler said what is in the data. The validator **counts** it. Every check returns a number, a
universe and the dimension it belongs to. Without that it is an impression, not a measurement.

Pipeline: `dq-profiler` → **dq-validator** → `dq-auditor`. Standard in `dq-pipeline`.
Full SQL catalogue: `references/checks-sql.md`.

## Measurement rules

- **A universe for every number.** `SUM(...)` without `COUNT(*)` and without a filter on entity type is
  useless. Format: absolute count + percentage + out of what.
- **Anti-join, not `NOT IN`.** `NOT IN (SELECT ...)` with a single NULL returns an empty set and the check
  silently passes. Always `LEFT JOIN ... WHERE x.KEY IS NULL`.
- **Collation-safe JOIN.** `CONVERT(a.COL USING utf8mb4) = CONVERT(l.CODE USING utf8mb4)`, otherwise Error
  1267 or a silent non-match.
- **Fix the snapshot.** Metrics are computed as of a date, not as of `CURDATE()`, otherwise they are not
  reproducible.
- **Zero is a result.** "0 orphans" is a positive finding and belongs in the report. But add whether it is
  enforced (an FK exists) or accidental (no FK); without an FK it is a state, not a guarantee.
- **A sample for every finding.** For every non-zero count, pull 10–20 concrete rows. The sample does the
  interpretation, not the aggregate. It is a working default for understanding the cause, **not a
  statistically representative sample**; when the sample has to support a conclusion about the whole, use a
  sampling methodology (discovery sampling per ISACA) and state which in the report.

## Procedure by table type

### Operational table: all six dimensions

1. **Completeness**: per column, with the universe filter; tell NULL / `''` / a placeholder apart.
2. **Syntactic correctness**: the mask and nothing more: regex, length, whitespace, case; only on non-empty
   values.
3. **Semantic correctness**: anti-join to the code list, ranges, sentinel values, **checksum (mod 11)**,
   values that semantically belong elsewhere (a company ID in the sector field, the whole address hierarchy
   in the municipality field). The checksum belongs here, not under syntax: it does not say what the value
   looks like, it says whether it can exist.
4. **Internal consistency**: derived vs. source attribute, mutually exclusive attributes, paired
   attributes, time ranges.
5. **External consistency**: orphans, link to a registry (measure the match rate).
6. **Uniqueness**: natural key, redundancy of the whole record, shared values across N entities.

The order is a working sequence (cheap checks before JOINs to a registry), not a norm; nobody prescribes a
binding order of measurement. Uniqueness is normally measured at the level of whole records through match
codes; the split into three cuts above is my extension, not a definition.

### Code list (LOV): its own quality + how it is used

A code list is not part of the universe, but **its defects distort the measurement of the universe**, so it
is audited too. Template (covers 90 % of findings):

```sql
-- quality of the code list itself
SELECT COUNT(*) total,
  SUM(CODE IS NULL OR TRIM(CODE)='')                       code_blank,
  SUM(VALUE IS NULL OR TRIM(VALUE)='')                     value_blank,
  SUM(DESCR IS NULL OR TRIM(DESCR) IN ('','NA'))           descr_empty,
  SUM(CODE <> TRIM(CODE))                                  code_whitespace,
  SUM(BINARY TRIM(CODE) <> BINARY UPPER(TRIM(CODE)))       code_not_upper,
  SUM(DEL_FLAG IS NULL OR TRIM(DEL_FLAG) NOT IN ('Y','N')) del_flag_invalid,
  SUM(VALID_FROM > VALID_TO)                               range_reversed,
  COUNT(DISTINCT TRIM(VALUE))                              distinct_value
FROM <lov>;

-- use of the code list by operations (both sides!)
SELECT l.CODE, l.VALUE, COUNT(a.<col>) uses
FROM <lov> l LEFT JOIN <operational> a
  ON CONVERT(a.<col> USING utf8mb4) = CONVERT(l.CODE USING utf8mb4)
GROUP BY 1,2 ORDER BY uses;                        -- 0 = dead code list entry

SELECT a.<col>, COUNT(*) cnt                       -- values outside the code list
FROM <operational> a LEFT JOIN <lov> l
  ON CONVERT(a.<col> USING utf8mb4) = CONVERT(l.CODE USING utf8mb4)
WHERE a.<col> IS NOT NULL AND l.CODE IS NULL GROUP BY 1 ORDER BY cnt DESC;
```

What to always check on code lists:

| Check | Why | Real finding |
|---|---|---|
| codes used in operations but missing from the code list | root cause = missing FK | address type `P` on 382,210 addresses (40 %) outside the code list; frequency codes `3` and `5` on 91,693 contracts |
| codes in the code list with no use | dead entry or the consequence of another defect | 239 of 240 country codes unused (because of the blanket `CZE`); product category 7 with no product or contract |
| duplicate `VALUE` under different `CODE`s | split statistics | `Dipl.tech.` under CODE 19 and 20; `M.S.` and `MSc` with the same description |
| empty description column | the data dictionary effectively does not exist | `DESC`/`DESCR` NULL or `'NA'` in 7 of 9 code lists |
| inconsistent name of the description column | generic ETL must handle both variants | 4× `DESC` vs 5× `DESCR` for an identical attribute |
| inconsistent `DEL_FLAG` | three implementations of one flag | `char(1)` Y/N vs `tinyint` 0/1 vs `char(4)` |
| inconsistent sentinel `VALID_TO` | different "infinities" complicate the validity filter | `2999-01-01` vs `2999-12-31` vs NULL |
| obsolete entries not deactivated | the code list does not reflect reality | dissolved states `ANT`, `SCG` active; renamed `SWZ`, `MKD`, `LBY` with the old name |
| typos in values | propagate into reports and UI | `Corespondence`, `HERZEGOWINA`, `Bacherol` |
| truncated values | `varchar(n)` too short for the full name | 9 of 43 ESA95 codes with a truncated `VALUE` |
| an obsolete classification as a whole | at odds with regulation | ESA95 (ESA2010 since 2014), OKEČ (CZ-NACE since 2008), UIR-ADR (RÚIAN since 2012) |

### Reference table (REF): fitness as a yardstick

For a reference registry do not ask "does it have errors", ask **"can it carry the role of a standard at
all?"**:

```sql
-- coverage: how many operational values the registry knows at all
SELECT COUNT(*) operational_total,
       SUM(r.KEY IS NOT NULL) in_registry,
       ROUND(100.0*SUM(r.KEY IS NOT NULL)/COUNT(*),2) coverage_pct
FROM <operational> p LEFT JOIN <ref> r ON <join condition>;
```

A registry also has **time validity**. For a person or company registry do not only check that the
identifier exists today, but that the subject was active **as of the transaction date**; that is exactly why
the registry holds dates of creation and dissolution. A contract signed with a subject that ceased to exist
two years earlier is a finding, not a typo.

Real findings: the company ID registry had 1,000 rows for 119,348 business clients (coverage < 1 %, 98.31 %
of clients outside the registry), an unusable stub, validation not possible. The address registry had
2.6 M rows, but 88.7 % without coordinates and 292,878 with an inverted sign; a sister table was missing
49,670 valid addresses (1.91 %), had 21,145 rows with a digit in the street field (an atomicity violation in
the reference source itself) and **by design contained no P.O. boxes**.

The last point is an important kind of finding: **a scope gap of the source**. A client address with a post
office box is formally valid, but against this registry it will always be invalid. That is not a defect of
the client data, and the report has to say so, otherwise 100 % of those rows are reported as errors.

## Domain validators (example: Czech Republic)

The following rules are tied to the Czech jurisdiction. **The structure of the check is universal** (format
→ checksum → cross-consistency with derived attributes; the first step measures syntactic correctness, the
second semantic correctness, the third internal consistency); replace the specific weights and lengths with
the equivalent of the target country. The mapping table is in `dq-pipeline`, Portability section.

| Attribute | Rule |
|---|---|
| **Birth number (rodné číslo)** | 9 or 10 digits after removing `/`. A 10-digit one is valid when `RC % 11 = 0`, or (historical exception) `first9 % 11 = 10` and the last digit is `0`. 9-digit ones (before 1954) have no check digit, validate the structure only. Month: `+50` for women, `+20`/`+70` allocation increment (since 2004). |
| **Company ID (IČO)** | 8 digits (pad with leading zeros). Mod 11: weights 8..2 on the first 7 digits, `remainder = sum % 11`, check digit `(11 - remainder) % 10`. |
| **Postcode (CZ)** | `^[0-9]{5}$` without spaces. Watch for `char(5)`: input with a space got truncated. |
| **E-mail** | `^[A-Za-z0-9._+%-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,24}$`. Check `#`, `&` and an inner space separately; they are not typos but a systematic character swap. |
| **Mobile (CZ)** | 9 digits after stripping `+420`/`00420`, first digit 6 or 7. |
| **Landline (CZ)** | 9 digits, first digit 2–5. A number with prefix 2xx/5xx recorded as a mobile = a semantic error, not a syntactic one. |
| **Country code** | ISO 3166-1 alpha-3, three capital letters. |

Placeholders are evaluated before the regex, but do not count them as syntactic errors: they are **phantom
values, i.e. missing values, and belong to completeness**. If you let them into the regex, you report one
gap twice and under the wrong characteristic: `''`, `NA`, `N/A`, `NULL`, `.`, `-`, `cizinec`,
`NEVYPLNENO`, `9999999999`, `0000000000`, `99999`, `00000`, a number made of one repeated digit.

## Writing to DQM_MDR

The metadata repository = `information_schema` extended with columns for the measured dimensions
(`COMPLETENESS`, `SYN_CORR`, `SEM_CORR`, `INT_CONS`, `EXT_CONS`). One row = one column of one table. The
score is a share between 0 and 1, computed **against the relevant universe**:

```sql
UPDATE DQM_MDR SET COMPLETENESS = (
  SELECT 1 - ( SUM(CASE WHEN PARTY_TYPE='P' AND (PARTY_FNAME IS NULL OR TRIM(PARTY_FNAME)='')
                        THEN 1 ELSE 0 END)
               / NULLIF(SUM(CASE WHEN PARTY_TYPE='P' THEN 1 ELSE 0 END), 0) )
  FROM PART_PARTY )
WHERE TABLE_NAME = 'PART_PARTY' AND COLUMN_NAME = 'PARTY_FNAME';
```

Keep the column names consistent with what your environment uses; next to the spelled-out names, the
five-letter codes `CMPLT`, `SNCOR`, `SMCOR`, `INCNS`, `EXCNS`, `UNQNS` are also in circulation.

**How full the MDR is** is a metric in itself. When the repository records 39 columns from 4 tables while
the database has 23 tables, that is a finding ("the MDR does not cover the universe") and a task, to extend
it. Do not call it *coverage*: in DQ vocabulary that term is taken by a contextual characteristic of data,
i.e. what share of the possible values an attribute contains at all.

Not every characteristic makes sense for every column. Do not fill in zero; leave NULL and say why. Zero is
a measured value meaning "nothing passes", not "not measured", and in an aggregate it would drag the score
down.

## Aggregating the score

The score per column is an intermediate result. It is aggregated in two steps:

1. **Attribute score = the minimum of its measured characteristics.** The limiting factor: 90 %
   completeness with 80 % syntactic correctness gives 80 % usability. The weakest characteristic decides,
   because the process fails on it; an average would hide it.
2. **Use score = the weighted average of those minimums** over the attributes the process relies on (per
   the usage matrix in `dq-auditor`). Not over every column of the table: an attribute cross-selling does
   not need should not affect its score.

**Do not compute one overall score for the database.** It weighs everything by the same measure and hides a
critical local defect in a sea of clean columns. When management wants one, deliver it only with a
breakdown by use and the name of the worst attribute.

## Output of the validator

Per table one section `## 3. Audit`, split exactly by dimension (Completeness, Syntactic correctness,
Semantic correctness, Uniqueness, Internal consistency, External consistency, Writing the DQ score). For
each: criterion, query, measured number, sample, one-sentence interpretation.

The interpretation must be concrete. "Runs SQL" or "processes data" is not an interpretation; write what is
being validated, how much of it failed and what it means for the business.

## Common mistakes

| Mistake | Consequence | Instead |
|---|---|---|
| A count without its universe | a number nobody can interpret | absolute count + percentage + out of what, with the entity-type filter |
| `NOT IN (SELECT …)` for orphans | one NULL empties the result and the check silently passes | anti-join `LEFT JOIN … WHERE key IS NULL` |
| `CURDATE()` inside a metric | the score changes on rerun | a fixed snapshot date |
| Counting placeholders as syntax errors | one gap reported twice, under the wrong characteristic | treat them as missing values under completeness |
| Writing 0 for a characteristic that does not apply | the aggregate score is dragged down by something never measured | leave NULL and say why |
| Averaging characteristics into an attribute score, or one score for the whole database | a critical defect disappears in clean columns | minimum per attribute, weighted average per use |

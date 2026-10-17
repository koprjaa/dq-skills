---
name: dq-imputer
description: Fills in missing values. Derivation from another attribute of the same record, aggregation from linked tables, lookup in a reference registry, a default category, statistical and model-based imputation, and above all the rules for when not to impute. Remediation step after dq-address, before dq-deduplicator. Use it when someone wants to "fill in missing values", "derive a date or gender", "impute", "enrich the data from a registry", "what to do with NULLs". Keywords: imputation, missing values, derivation, derived column, lookup, data enrichment, MCAR, MAR, MNAR, missingness mechanism, Buck's method, hot deck.
---

# Imputer: filling in missing values

Fill in only what can be **derived with certainty** or looked up in an authoritative source. An estimate
stored in a production table then looks exactly like a fact, and nobody can tell the difference any more.

Pipeline: `dq-address` → **dq-imputer** → `dq-deduplicator`.

## Order of reliability

| # | Method | Reliability | Use |
|---|---|---|---|
| 1 | **derivation from another attribute of the same record** | deterministic | the identifier carries the date, gender, check digit |
| 2 | **aggregation from a linked table** | deterministic | start of the relationship = minimum over the transactions |
| 3 | **lookup in a reference registry** | high | sector, industry classification, legal form, municipality by address code |
| 4 | **a default category for a whole class of entities** | methodological | "a natural person belongs to the household sector" |
| 5 | **no model** (mean, median, mode, midrange) | low | distorts the distribution, see below |
| 6 | **implicit model** (Hot Deck, Cold Deck, k-NN) | medium, but traceable | analytical layer, with a flag |
| 7 | **explicit model** (Buck's method, logistic regression, Naive Bayes, trees, MCMC) | lowest | modelling and research |

Methods 1–4 (database techniques, derivation from other values) go into production without discussion. Beyond
that, **explainability decides, not whether the method is statistical**:

- A robust and auditable method (Buck's, a conditional mean from regression; Hot Deck) can get into operations,
  with a flag and a documented procedure.
- A black box (AutoML, k-NN over dozens of variables, a neural network) cannot. When an institution refuses a
  client a service based on scoring, it must be able to explain where every input value came from. What you
  cannot defend before a regulator does not belong in operations.

## Iron rules

- **Never impute through an invalid source.** Derive only from a value that passed validation (checksum, not
  just format). A date of birth computed from a birth number with a broken checksum is an invented date with the
  face of a fact.
- **Never overwrite a filled value.** Imputation only fills NULLs. A collision (the stored value differs from the
  derived one) is *a finding for the audit*, an internal consistency defect, not a reason to overwrite. Which of
  the two sources is more credible is decided outside the imputer, with justification.
- **Always into a `_STD` column**, never into the original. The original NULL must stay visible.
- **Mark the origin.** Either a separate flag (`_IMPUTED`) or a separate column. Without it you cannot count how
  much data is measured and how much is inferred.
- **Sanity check after imputation.** Range, distribution, extremes, and not just the technical range of the type
  but **business constraints**: a date of birth not in the future, a policyholder's age above the age of legal
  capacity, a contract date within the product's lifetime. Deriving the year from two digits without the right
  century can produce clients born in 2074.
- **Measure before/after.** The attribute's completeness before imputation, after imputation, and how much
  remains; that is the main output of this step.

## 1. Derivation from another attribute

A person identifier often carries the date of birth and gender. The pattern (Czech birth number; in another
domain replace it with the equivalent identifier):

```sql
ALTER TABLE PART_PARTY ADD PARTY_DOFBIRTH_STD date NULL, ADD PARTY_GENDER_STD char(1) NULL;

-- 1) carry over existing values
UPDATE PART_PARTY SET PARTY_DOFBIRTH_STD = PARTY_DOFBIRTH WHERE PARTY_DOFBIRTH IS NOT NULL;

-- 2) fill only where it is missing AND the source is valid
UPDATE PART_PARTY SET PARTY_DOFBIRTH_STD = STR_TO_DATE(CONCAT(
    CASE WHEN LENGTH(TRIM(PARTY_RC))=10 AND SUBSTR(PARTY_RC,1,2)*1 < <pivot>
         THEN CONCAT('20',SUBSTR(PARTY_RC,1,2)) ELSE CONCAT('19',SUBSTR(PARTY_RC,1,2)) END,'-',
    CASE WHEN SUBSTR(PARTY_RC,3,2)*1 > 12 THEN SUBSTR(PARTY_RC,3,2)*1 - 50
         ELSE SUBSTR(PARTY_RC,3,2)*1 END,'-', SUBSTR(PARTY_RC,5,2)), '%Y-%m-%d')
WHERE PARTY_TYPE='P' AND PARTY_DOFBIRTH_STD IS NULL AND RC_VALID = 1;

-- 3) sanity check: nonsensical results back to NULL
UPDATE PART_PARTY SET PARTY_DOFBIRTH_STD = NULL WHERE YEAR(PARTY_DOFBIRTH_STD) < 1900;
```

The century pivot is a trap. The nine-digit variant is always 19xx; for the ten-digit one decide against the
snapshot date (`2000+yy <= snapshot_year` → 2000s, otherwise 1900s), not against a fixed constant.

Gender from the same source:

```sql
UPDATE PART_PARTY
SET PARTY_GENDER_STD = CASE WHEN SUBSTR(PARTY_RC,3,2)*1 BETWEEN 51 AND 62 THEN 'F' ELSE 'M' END
WHERE PARTY_TYPE='P' AND PARTY_GENDER_STD IS NULL AND RC_VALID = 1;
```

**Keep the original value, fill only what is missing.** When the stored gender differs from the derived one, it
is a finding for `dq-auditor`, not a task for the imputer.

## 2. Aggregation from a linked table

Start of the relationship = the earliest evidence of the relationship. Take the minimum of both sources, not just
a fallback:

```sql
ALTER TABLE PART_PARTY ADD PARTY_SINCE_STD date NULL;
UPDATE PART_PARTY SET PARTY_SINCE_STD = PARTY_SINCE WHERE PARTY_SINCE IS NOT NULL;

UPDATE PART_PARTY a SET a.PARTY_SINCE_STD = LEAST(
    COALESCE(a.PARTY_SINCE_STD, '9999-12-31'),
    COALESCE((SELECT MIN(b.CNTR_VALIDFROM) FROM PROD_CONTRACT b WHERE b.PARTY_ID=a.PARTY_ID),
             '9999-12-31'))
WHERE a.PARTY_SINCE_STD IS NOT NULL OR EXISTS
      (SELECT 1 FROM PROD_CONTRACT b WHERE b.PARTY_ID = a.PARTY_ID);
UPDATE PART_PARTY SET PARTY_SINCE_STD = NULL WHERE PARTY_SINCE_STD = '9999-12-31';
```

This also fixes records where the relationship date was later than the first transaction, a logical nonsense
that filling NULLs alone does not resolve.

Watch sentinel dates in aggregation: `MAX(valid_to)` over `2999-12-31` returns the sentinel. Filter them **before**
the aggregation, or detect them from the string; in pandas a `Timestamp` ends in the year 2262 and `2999` silently
flips to `NaT`:

```python
yr = pd.to_numeric(contr["CNTR_VALIDTO"].astype(str).str[:4], errors="coerce")
open_ended = yr >= 2200  # open-ended validity, not a missing date
vt = pd.to_datetime(contr["CNTR_VALIDTO"].where(~open_ended), errors="coerce")
```

## 3. Lookup in a reference registry

The most valuable imputation: the value comes from an authoritative external source.

```sql
-- industry classification and legal form by company identifier
UPDATE PART_PARTY a INNER JOIN REF_RES r ON r.ICO = TRIM(a.PARTY_CREGNUM)
SET a.PARTY_NACE_STD = r.NACE, a.PARTY_FORM_STD = r.ROSFORMA
WHERE a.PARTY_TYPE='C' AND a.PARTY_NACE_STD IS NULL;
```

Two things about registries accessed through an API: they have **quotas** (thousands of requests a day, fewer at
peak), so batch and cache to avoid hitting the limit in the middle of a run. And take the value **as of the date
you need it**: today's sector or legal form need not apply to a contract from 2012.

Before linking, check **the registry's coverage** (see `dq-validator`). A registry with 1,000 rows for 119,348
subjects fills in nothing useful and is a finding in itself; the right recommendation is not "impute" but
"connect to the real registry".

From the address code (`dq-address`) you can look up a missing postcode or municipality. A truncated postcode
(`251 6`) is fixed **this way**: by lookup, not by calculating the missing digit.

## 4. A default category for a whole class of entities

When a classification has a category a class of entities belongs to by definition:

```sql
UPDATE PART_PARTY SET PARTY_ESA95_STD = 14000       -- Households
WHERE PARTY_TYPE='P' AND PARTY_ESA95_STD IS NULL;
```

That is not an estimate but a methodological completion, and it still belongs in the report with its
justification. The difference from a blanket constant (see `dq-profiler`) is that here the value is **derived from
the entity type**, not assigned to everyone regardless. Check it: after imputation, 100 % of records must not have
the same value.

## 5. Statistical and model-based imputation

### First the mechanism, then the method

Before reaching for statistics, determine **the missingness mechanism**. It chooses the method, not habit:

| Mechanism | What it means | Consequence |
|---|---|---|
| **MCAR** | missing with the same probability everywhere, independent of anything | statistical imputation is defensible |
| **MAR** | the probability of missing can be predicted from other variables | conditional methods (Buck's) give a consistent estimate |
| **MNAR** | missing depends on the value itself (selective filling) | do not impute statistically; it is a finding about the process |
| **MBND** | the value cannot exist by nature (number of pregnancies for a man) | do not impute at all, fix the universe or the model |

MCAR is **tested, not guessed**: Little's MCAR test, or t-tests between the group with the value and without it.
MAR versus MNAR cannot be told apart mathematically without additional information, and that alone is a reason
for caution.

A missingness matrix (`missingno`) is a cheap first look: a systematic gap across a whole class of entities, one
period or one channel shows up immediately.

### Two traps

- **Distorted distribution.** Replacing missing values with one constant (mean, median) produces an unnatural peak
  in the middle of the histogram. Variance drops, correlations drift and conclusions from such data do not hold,
  even though the table looks complete.
- **The imputation paradox.** A model trained on data filled in by an explicit model shows *better* metrics (lower
  MAE) than a model on the original complete data. It is not better: imputation introduced artificial
  multicollinearity into the data and fitted it. **Treat an improved metric after imputation as a warning, not a
  success.**

## What never to impute

| Case | Why |
|---|---|
| a person or entity identifier | an invented identifier is worse than a missing one; it blocks verification and compliance, and for personal data also runs into GDPR |
| a value derived from an invalid source | carries the error forward and masks it |
| a 100 % empty column | there is nothing to derive from; decide between "fill from a registry" and "drop the column" |
| a missing relationship (orphan) | assigning it to a random parent is fabrication; either look it up or delete it |
| a status/termination type with no code list | nothing to validate against; first the code list, then filling |
| a blanket constant instead of the real value | that is exactly how the original defect arose |

## Output of the imputer

- `_STD` columns with filled values + an origin flag.
- **A before/after table per attribute**: completeness before, after, how much was filled by which method, how
  much remains and why it could not be done.
- A list of collisions (stored ≠ derived value) → back to `dq-auditor` as an internal consistency finding.

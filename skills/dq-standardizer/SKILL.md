---
name: dq-standardizer
description: Standardises data into canonical form and links it to reference dictionaries. Titles, first and last names against a registry, national and company IDs, postcode, e-mail, phone, diacritics, case, padding, placeholders to NULL, the convention of _STD columns and match keys. Second remediation step, after dq-parser, before dq-imputer. Use it when someone needs to "unify how values are written", "standardise values", "link to a code list", "normalise names or titles", "clean up e-mails and phones". Keywords: standardisation, unification, normalisation, canonical form, reference dictionary, diacritics, match key, _STD columns, value cleansing.
---

# Standardiser: canonical form

Second remediation step. The data is already atomic (`dq-parser`). Now every attribute gets **one canonical
form** and a link to a reference dictionary. Without it neither imputation nor deduplication can run: `MuDr`
and `MUDr.` are two different values to a machine.

Pipeline: `dq-parser` → **dq-standardizer** → `dq-address` → `dq-imputer` → `dq-deduplicator`.

## Conventions

- **A new column `<ATTRIBUTE>_STD`, the original never changes.** Before/after is evidence for the report and
  the only way back when a rule turns out wrong.
- **Every fix is a registered rule**, not an ad hoc `UPDATE`. A rule has a name, a description of the pattern,
  the number of rows affected and a reference into the metadata repository. That includes deterministic fixes
  that look trivial (a character swap from a migration); that is why `dq-parser` does not fix them, it only
  flags them. Without a rule register you cannot trace what changed a value between the original and `_STD`.

```sql
ALTER TABLE PART_PARTY
  ADD PARTY_RC_STD      varchar(10), ADD PARTY_FNAME_STD varchar(45),
  ADD PARTY_MNAME_STD   varchar(45), ADD PARTY_LNAME_STD varchar(45),
  ADD PARTY_TITBEF_STD  varchar(10), ADD PARTY_TITAFT_STD varchar(10);
```

- **Two different outputs from one value**; do not mix them up. Methodologically they have two different
  names, and the difference is substantial, not hair-splitting:

| Output | Methodological name | What for | What it looks like |
|---|---|---|---|
| **display** (`_STD`) | **standardisation** | display, correspondence, reporting, export | `Nováková`, `MUDr.`, `Praha 5` |
| **match key** (`_MATCH`) | **unification** | comparison, dedup, JOIN to a registry | `NOVAKOVA`, `mudr`, `praha5` |

Standardisation raises the quality of a value for people and for reporting. Unification, in contrast,
**flattens** the value so an algorithm can compare it, and on its own does not raise quality. Writing
"standardisation" where you mean clustering duplicates is a methodological error.

Match key (unified form): no diacritics, uppercase (or lowercase, as long as it is consistent), no punctuation
or spaces. The display form **keeps** diacritics and punctuation; otherwise you produce correspondence
addressed to "Novakova".

- **Standardisation must not change the match key**, otherwise the clusters in deduplication reshuffle. Fixing
  diacritics `MARTÍNKOVA` → `MARTÍNKOVÁ` changes the display; the match key stays `MARTINKOVA`.

- **Take formats from ISO, not from memory.** Date and time `ISO 8601` (`YYYY-MM-DD`), currency `ISO 4217`
  (`CZK`, `USD`), country `ISO 3166-1`. A home-made format means a home-made parser at every consumer.

- **Naming by the canonical model.** Suffixes `_STD` (cleaned value), `_FLAG` / `_IND` (flag), `_DATE` (date);
  prefixes `LOV_` (code list), `REF_` (registry), `V_` (view). The convention is not cosmetic; generic ETL and
  monitoring rely on it.

## Placeholders → NULL (before anything else)

A placeholder is not data. Before you standardise, convert it to NULL; otherwise `9999999999` becomes a "valid
birth number" for thousands of clients and `NA` becomes a surname.

```sql
UPDATE PART_PARTY SET PARTY_RC = NULL
WHERE TRIM(LOWER(COALESCE(PARTY_RC,''))) IN
      ('','cizinec','nevyplneno','nevyplněno','9999999999','0000000000');

UPDATE PART_PARTY SET PARTY_FNAME = NULL, PARTY_LNAME = NULL
WHERE PARTY_TYPE='C' AND TRIM(UPPER(COALESCE(PARTY_FNAME,''))) IN ('NA','N/A','NULL','.');
```

Formally this is **disguised missing data**: there is a value in the field but no data behind it, so profiling
reported falsely high completeness.

Count and record how many there were; it is before/after evidence and at the same time a correction of the
completeness metric (completeness drops after the conversion, and that is right: it was falsely inflated
before).

## Domain rules: a worked example (Czech Republic)

The following sections up to Padding are **a worked example for the Czech jurisdiction**, not a universal
prescription. The structure is portable, not the constants:

| Universal procedure | In the example below |
|---|---|
| person identifier: strip → format → checksum → two flags | birth number, mod 11 |
| company identifier: pad to a fixed length, text type | IČO, `char(8)` |
| postal code: canonical form, text type, no inferring | postcode, 5 digits |
| phone: strip → national prefix → length → type by prefix | +420, mobile 6/7, landline 2–5 |
| names: display vs. match, fix against a name registry | feminine surname endings, Czech diacritics |

Equivalents for other jurisdictions are in the mapping table in `dq-pipeline`, Portability section. When you
change country, change the constants and registries; the order of steps stays.

### Person identifier (birth number)

```sql
UPDATE PART_PARTY SET PARTY_RC_STD = REPLACE(TRIM(PARTY_RC),'/','')
WHERE PARTY_TYPE='P' AND REPLACE(TRIM(PARTY_RC),'/','') REGEXP '^[0-9]{9,10}$';
```

Next to `_STD` keep **two flags**, not one; it is the most important decision in the whole standardisation:

| Flag | Meaning | What for |
|---|---|---|
| `RC_USABLE` | readable structure (month 1–12 after subtracting 50/20/70, day 1–31) | internal derivation of the dedup key |
| `RC_VALID` | also passed the mod 11 checksum | **output**: the published value, derived date of birth and gender |

Let only `RC_VALID` into the export and the derived attributes. `RC_USABLE` serves only as a dedup signal
inside the pipeline: a readable but checksum-broken birth number still reliably identifies the same person in
two records, but it must not be published.

The pair of flags is my extension: the stricter variant (and the default in academic methodology) stores only
fully valid values in `_STD`. There is no contradiction: an invalid value does not get into `_STD` here either;
I just keep the information that it is *readable* next to it, because without it you lose part of the dedup
signal.

### Company identifier (IČO)

```sql
-- pad with leading zeros to 8, drop non-digits
UPDATE PART_PARTY SET PARTY_CREGNUM_STD = LPAD(REGEXP_REPLACE(PARTY_CREGNUM,'[^0-9]',''),8,'0')
WHERE PARTY_TYPE='C' AND LENGTH(REGEXP_REPLACE(PARTY_CREGNUM,'[^0-9]','')) BETWEEN 1 AND 8;
```

Never store a company ID in a numeric type; you lose the leading zeros and `00123456` becomes `123456`. The
right type is `char(8)`: the length is guaranteed, so it is an exception to the rule "replace char with
varchar" (`dq-guard`), which applies to variable-length text, not to a fixed-length code.

Padding with zeros to eight characters is not cosmetic: state organisational units, for example, have shorter
IDs and without the padding they do not link to the registry.

### Titles: linking to a reference dictionary

The pattern that works: **normalise both sides the same way** (lower + remove dots + trim), join through the
normalised form, but store the **canonical value from the registry**.

```sql
UPDATE PART_PARTY a
INNER JOIN REF_TITBEF b
  ON LOWER(REPLACE(TRIM(CONVERT(a.PARTY_TITBEF USING utf8mb4)),'.','')) =
     LOWER(REPLACE(TRIM(CONVERT(b.VALUE        USING utf8mb4)),'.',''))
SET a.PARTY_TITBEF_STD = b.VALUE;
```

This one step moves the match with the code list from ~21–29 % to ~81 %. The rest are real problems, not
spelling variants, and fall into three bins:

| Bin | Example | What to do |
|---|---|---|
| old/regional spelling | `DrS` vs `DrSc.`, `PhD` vs `Ph.D.` | a manual mapping table of historical forms |
| semantic noise | `otec` (father) in the title field (2,118 rows) | to NULL, plus a finding: the field is used for another purpose |
| missing from the registry | a legitimate form the dictionary does not know | add it to the registry, do not bend the data |

Watch the registry itself: it can contain a duplicate value under two codes (`Dipl.tech.` under CODE 19 and 20)
or a typo in the description (`Bacherol`). Then the link is ambiguous; merge the codes in the registry before
you link operations to it.

### First and last names

Three steps, in this order:

**1. Basic clean + display/match split**

```python
display = " ".join(w.capitalize() for w in s.split())
match = re.sub(r"[^A-Z0-9 ]", "", unidecode(s).upper()).strip()
```

**2. Registry-based recovery of a lost letter.** Reconstruct a corrupted surname (`BE0ŠOVÁ`) against the name
registry: replace `0` with a wildcard `.{1,2}` and look for a match; with several candidates take the most
frequent one. When the registry does not know the diacritic variant, try a fallback without diacritics.

```python
pat = "^" + "".join(".{1,2}" if ch == "0" else re.escape(ch) for ch in disp.upper()) + "$"
cand = ref[ref["U"].str.fullmatch(pat, na=False)]
if len(cand) == 0:  # fallback without diacritics
    patA = "^" + "".join(".{1,2}" if ch == "0" else re.escape(ch) for ch in unidecode(disp.upper())) + "$"
    cand = ref[ref["UASC"].str.fullmatch(patA, na=False)]
best = cand.sort_values("FREQ", ascending=False).iloc[0]["VALUE"] if len(cand) else None
```

**3. Standardise the display form against the registry** (fixing diacritics and phonetics: `MARTÍNKOVA` →
`MARTÍNKOVÁ`, `TÚMOVÁ` → `TŮMOVÁ`). Map through an accent-insensitive key to the most frequent variant in the
registry. The match key does **not change**, so dedup stays the same.

A deterministic feminine ending is fixed **before** the registry: the registry also contains junk forms
without diacritics, so an exact match would return the wrong variant again:

```python
if is_lname and disp.endswith("ova"):
    disp = disp[:-1] + "á"
```

What the registry does not fix stays corrupted with a flag; `dq-deduplicator` merges it later from a clean
twin of the same person in the cluster.

### E-mail

```python
s = str(v).strip().lower()
s = s.replace("#", "@").replace("&", ".")  # systematic swap from the import
s = re.sub(r"\s+", "", s)  # spaces inside the address
valid = bool(re.fullmatch(r"[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,}", s))
```

Those two `replace` calls are legitimate only because the swap is **proven systematic** (22,694 rows of one
pattern), not typos. Verify it by frequency first, otherwise you manufacture falsely valid addresses. Let only
`valid` into the export.

### Phone (Czech numbering plan)

```python
d = re.sub(r"\D", "", str(v))
d = re.sub(r"^00420", "", d)
d = re.sub(r"^420", "", d) if len(d) > 9 else d
if len(d) != 9 or d == d[0] * 9:
    return (None, None)  # 999999999 = bogus
if d[0] in ("6", "7"):
    return (d, None)  # mobile
if d[0] in ("2", "3", "4", "5"):
    return (None, d)  # landline
```

The mobile/landline split is not cosmetic: a landline recorded as a mobile is why the SMS channel silently does
not work. Standardisation is at the same time a **reclassification of the contact type**.

### Postcode

```sql
UPDATE PARTY_ADDRESS SET ADDR_ZIP_STD = REGEXP_REPLACE(ADDR_ZIP,'[^0-9]','')
WHERE REGEXP_REPLACE(ADDR_ZIP,'[^0-9]','') REGEXP '^[0-9]{5}$';
```

**Do not complete** a truncated postcode (`251 6` from the original `251 62`) by guessing; the last digit is
missing. It is resolved by a lookup in the registry through municipality + street (`dq-imputer`), not by
calculation.

Store it as `varchar`, not `integer`: a numeric type kills the leading zero and foreign alphanumeric codes.

## Padding and case

```sql
UPDATE <t> SET <col> = NULLIF(TRIM(<col>), '');
```

Remove `char(n)` padding globally first, otherwise every later comparison needs `TRIM` and somewhere it gets
forgotten. At the same time propose changing the type to `varchar`; otherwise the padding returns with the next
write (that belongs in `dq-guard`).

## When not to standardise

- **Over-standardisation is also a defect.** Unifying everything to one form kills a semantic detail someone
  needs: a local variant of a name, a historical spelling, a distinction that disappears in the canonical form.
  Standardise what has a consumer. Leave the rest.
- **The degree of fixing follows use.** An operational system needs a control at input; regulatory reporting
  needs absolute formal agreement with the prescribed model; training data for a model needs robustness, where a
  little noise is cheaper than aggressive cleaning that wipes out signal.
- **No automatic method is a hundred percent.** Every blanket replace also produces false positives. For
  correspondence and personal data that is not statistics but legal and reputational risk: a wrongly merged
  correspondence address can deliver a life insurance letter to someone who was not supposed to know about the
  policy. Above the confidence threshold, automation; below it, a **semi-automatic mode with escalation** to a
  steward or salesperson. Never a silent write.
- **Cleaning treats a symptom.** Until the rule reaches the application's input, the defect returns with the
  next load, the *Gemba Kaizen* principle: fix the process where the defect arises, not its result in the data
  warehouse. Prevention is handled by `dq-guard`.

## Output of the standardiser

- `_STD` columns, `_MATCH` keys, validity flags (`RC_VALID`, `EMAIL_VALID`, `_CORRUPT`).
- **A before/after table for every attribute**: how many values changed, how much the match with the registry
  rose, how many remained outside and why.
- A list of values standardisation did not resolve: input for extending the registry or for manual review.

## Common mistakes

| Mistake | Consequence | Instead |
|---|---|---|
| Standardising before placeholders are NULL | `9999999999` becomes a valid identifier, `NA` a surname | convert placeholders to NULL first and count them |
| An ad hoc `UPDATE` instead of a registered rule | nobody can trace what changed a value | every fix has a name, pattern, row count and MDR reference |
| One form for display and matching | correspondence addressed without diacritics, or dedup that misses duplicates | `_STD` for display, `_MATCH` for comparison |
| A display fix that changes the match key | clusters reshuffle in deduplication | the match key stays stable |
| A blanket replace that is not proven systematic | falsely valid values | verify the pattern by frequency first |
| Completing a truncated postcode by guessing | an invented digit | look it up in the registry (`dq-imputer`) |

---
name: dq-parser
description: Parsing and atomising data. Splits composite values into dedicated columns (house numbers out of the street, municipality vs. city district, first and middle name, title from name), fixes interleaved spaces and encoding errors, extracts patterns with regex. First remediation step, after the audit, before dq-standardizer. Use it when "house numbers are merged into the street", "composite values sit in one field", "atomicity is broken", "the address or name needs parsing". Keywords: parsing, atomisation, atomicity, address splitting, house number, regexp_substr, split, extraction.
---

# Parser: atomisation

First remediation step. **Do not standardise or unify anything until it is atomic.** A field `ADDR_STREET`
with the value "Na Budíně 854" cannot be linked to a registry that knows the street and the number
separately.

Broken atomicity is not cosmetic, it is a **violation of first normal form**: without atomic values the
model has no reliable record identifier, and any approximate matching and merging with another source is
unreliable.

Pipeline: audit → **dq-parser** → `dq-standardizer` → `dq-address` → `dq-imputer` → `dq-deduplicator`.

## Rules

- **The original is never deleted.** Parsed output goes into new columns (`_STD`, `_NUM1`, `_NUM2`). The
  original is evidence for before/after and a safety net when the parser gets it wrong.
- **Deterministic, not heuristic.** A parser that guesses wrong in 3 % of cases produces silent damage. When
  you are not sure of a rule, leave NULL and count how many remained. This holds for structured fields with a
  known pattern: address, name, date. For free text, where no pattern exists, statistical entity recognition
  (NER, spaCy) is a legitimate tool, but its output is an estimate: it comes with a confidence score and a
  flag, and does not go into a production column without review.
- **Chop disallowed characters before you split.** For every semantic type keep a table of allowed characters
  (a *chop table*): digits for a postcode, alphanumerics for an IBAN; strip the rest. It raises the success of
  splitting more than a more complex regex.
- **Measure the parse rate**: how many rows were split successfully. That is a metric for the report; do not
  confuse it with the *match rate*, which is measured only when linking to a registry (`dq-address`).
- Parse **before** anything else: standardisation, imputation and deduplication all stand on atomic fields.

A regex solves one pattern. Industrial parsers use a **grammar** of the semantic type stored in a knowledge
base (QKB): it describes the expected structure of an address or name and assigns a probability of occurrence
to each combination of tokens. When you are writing the fifth regex for the same entity, you are missing a
grammar, not another regex.

## Address

The most common defect: the dedicated columns for house numbers are 100 % empty and the number is merged into
the street name. In the source it looks like `Na Budíně 854`, `Nádražní 1211/25a`, `Bezručova 12/3`.

Split into three parts: street name, descriptive number (číslo popisné), orientation number (číslo
orientační, possibly with a letter):

```sql
-- MySQL 8: REGEXP_SUBSTR / REGEXP_REPLACE
UPDATE PARTY_ADDRESS SET
  ADDR_NUM1_STD   = REGEXP_SUBSTR(ADDR_STREET, '[0-9]+(?=(/[0-9]+[a-zA-Z]?)?[[:space:]]*$)'),
  ADDR_NUM2_STD   = REGEXP_SUBSTR(ADDR_STREET, '(?<=/)[0-9]+[a-zA-Z]?[[:space:]]*$'),
  ADDR_STREET_STD = TRIM(REGEXP_REPLACE(ADDR_STREET, '[0-9]+(/[0-9]+[a-zA-Z]?)?[[:space:]]*$', ''));
```

Python equivalent for a pandas pipeline:

```python
num = raw.str.extract(r"(\d+)(?:/\d+\w?)?\s*$", expand=False)  # descriptive number
street = raw.str.replace(r"\d+(/\d+\w?)?\s*$", "", regex=True).str.strip()
```

Watch the order of the numbers: in the Czech Republic the form is `descriptive/orientation`, but the
convention is not always followed. When the parser finds only one number, store it as the descriptive number
and leave the orientation number NULL; do not guess.

### Composite value in the municipality field

`Praha 5 - Smíchov`, `České Budějovice - České Budějovice 5`: the operator typed the whole address hierarchy
(municipality + city district + district number) into one field. It makes matching against the registry
impossible.

```sql
-- the first segment before the dash = municipality, the rest = city district
SELECT ADDR_CITY,
  TRIM(SUBSTRING_INDEX(ADDR_CITY, '-', 1))                      municipality,
  NULLIF(TRIM(SUBSTRING(ADDR_CITY, LOCATE('-',ADDR_CITY)+1)),'') district
FROM PARTY_ADDRESS WHERE ADDR_CITY LIKE '%-%';
```

Detect candidates before parsing:

```sql
SELECT ADDR_CITY, COUNT(*) cnt FROM PARTY_ADDRESS
WHERE ADDR_CITY REGEXP '[0-9]' OR ADDR_CITY LIKE '%-%' OR LENGTH(ADDR_CITY) > 50
GROUP BY 1 ORDER BY cnt DESC LIMIT 20;
```

Beware: a dash in a municipality name is sometimes legitimate (`Frýdek-Místek`, `Brandýs nad Labem-Stará
Boleslav`). So the split is verified against the registry of municipalities, not blindly; when the
municipality is not in the registry after the split, restore the original value.

### P.O. boxes and other out-of-scope forms

A P.O. box, a post office box, "at the company's delivery address": formally valid addresses the building
registry does not know. Do not force-parse them; mark them with their own flag and exclude them from the
match rate metric (otherwise you report as an error something that is not one).

## Name

### Interleaved spaces

`D A V I D`, `Ž A N N A`: every letter separated by a space. They can be joined deterministically when **all**
tokens are single letters, or when most of them are (the rest is typically a hyphen or an initial):

```python
toks = s.split()
if len(toks) > 1 and all(len(t) == 1 for t in toks):
    s = "".join(toks)
elif len(toks) > 1 and sum(len(t) == 1 for t in toks) >= max(2, len(toks) - 1):
    s = "".join(toks)
```

Never join tokens in general: `Anna Marie` is two names, not `AnnaMarie`.

### Multi-part name

First + middle name in one field. Split on the space and store the second token separately; drop
single-letter tokens (initials), otherwise you produce a meaningless middle name:

```sql
UPDATE PART_PARTY
SET PARTY_FNAME_STD = <proper>(SUBSTRING_INDEX(PARTY_FNAME,' ',1)),
    PARTY_MNAME_STD = NULLIF(<proper>(SUBSTRING_INDEX(SUBSTRING_INDEX(PARTY_FNAME,' ',2),' ',-1)),
                             <proper>(SUBSTRING_INDEX(PARTY_FNAME,' ',1)))
WHERE PARTY_TYPE='P' AND LENGTH(SUBSTRING_INDEX(PARTY_FNAME,' ',2)) > 1;
```

### Title written in the name field

`Ing. Jan Novák` in `PARTY_NAME`. Detect it by matching the first token against a dictionary of titles, not by
the dot: `J. Novák` is an initial, not a title.

The dictionary is the code list of titles (`REF_*_TITBEF` for titles before the name, `TITAFT` after it), and
the match is fuzzy, because `MuDr`, `MUDr.` and `Mudr` sit side by side in the data. The parser only separates
the title; converting it to the canonical form is work for `dq-standardizer`.

### Encoding errors

Two different things, two different solutions:

| Error | Example | Solution |
|---|---|---|
| systematic character swap | `ĄUDOVÍT` (should be `ĽUDOVÍT`) | the pattern is unambiguous (`Ą` at the start of a word is always a bad transcription), but the fix belongs in `dq-standardizer` as a registered rule. In the parser only when that character blocks the split itself |
| lost letter | `BE0ŠOVÁ` (should be `BENEŠOVÁ`) | **do not fix it in the parser.** Mark it with a `_CORRUPT` flag and leave the reconstruction to `dq-standardizer` (name registry) or `dq-deduplicator` (a clean twin in the cluster) |

```python
s = re.sub(r"(^|(?<=\s))Ą", "Ľ", s)  # an unambiguous rule, but it belongs in standardisation
corrupt = bool(re.search(r"[0-9]", s))  # a digit in a name = corrupted, handled elsewhere
```

**The parser splits, it does not fix.** Even a deterministic fix belongs in standardisation as a registered
rule; otherwise no audit trail remains and nobody can trace what changed the value. The only exception is a
character that blocks the split itself. Reconstructing a missing letter is also an estimate and belongs where
there is something to verify it against.

## Date stored as text

Before parsing, check the format and day/month:

```sql
SELECT SUBSTRING(<col>,1,2) first, SUBSTRING(<col>,4,2) second, COUNT(*)
FROM <t> GROUP BY 1,2 ORDER BY 3 DESC LIMIT 20;
```

When the first position never exceeds 12 and the second does, the order is `MM.DD`; when both exceed 12, the
format is mixed and has to be decided per row from a second source. A blind `STR_TO_DATE` will not catch the
silent swap and spreads it across the whole table.

## Output of the parser

- New `_STD` columns with atomic values, originals untouched.
- The parse rate for each part (`% of rows with a non-empty result`).
- **An error table** of rows the parser did not split, with the reason for each: the audit trail of why a
  record did not pass. Input for manual review or for the decision "this is out of scope".

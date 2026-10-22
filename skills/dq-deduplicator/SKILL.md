---
name: dq-deduplicator
description: Deduplication and master data management. Builds a match code (MCODE) with a key hierarchy, clusters duplicate records, picks the survivor record, consolidates the golden record across the cluster, identifies households, scores record quality and compares key variants, plus probabilistic record linkage (Fellegi-Sunter, blocking, thresholds and a grey zone). The last remediation step before dq-guard. Use it when someone wants to "deduplicate clients", "find duplicates", "golden record", "single version of truth", "merge records", "identify households", "MDM". Keywords: deduplication, MCODE, match code, golden record, survivor, household, MDM, cluster, single customer view, record linkage, Fellegi-Sunter, splink, blocking key, similarity threshold.
---

# Deduplicator: golden record

The last remediation step. The goal: **one entity = one record**, plus a provable key that the cluster can be
reconstructed from at any time.

Pipeline: `dq-imputer` → **dq-deduplicator** → `dq-guard`.

Vocabulary for an academic audience: cluster = **match group**, MCODE = **match code**, golden record =
**consolidated client** (Single Customer View, a unified customer base).

**Do not do this without the previous steps.** Dedup on unstandardised data finds a fraction of the duplicates
and the measured number is falsely low. `Nováková` and `NOVAKOVA` are two people to an exact match.

## Match code (MCODE): key hierarchy

The MCODE is a hash of the identity, not of the row. Build it **as a cascade from the strongest key**; the first
one that can be computed wins:

| Order | Entity type | Key | Condition |
|---|---|---|---|
| 1 | person | `MD5('RC' + identifier)` | the identifier passed the **checksum**, not just the format |
| 2 | person | `MD5('FO' + birth_date + gender + first_name_match + last_name_match)` | a composite, when the identifier is missing or unreadable |
| 3 | person | `MD5('SINGLE' + PK)` | cannot be clustered → its own cluster of size 1 |
| 1 | company | `MD5('ICO' + identifier)` | a valid entity identifier |
| 2 | company | `MD5('PO' + name_match)` | without an identifier |
| 3 | company | `MD5('SINGLE' + PK)` | fallback |

```python
def mcode(r):
    if r["PARTY_TYPE"] == "P":
        if r["RC_VALID"] and r["RC_STD"]:
            return md5("RC", r["RC_STD"])
        if r["DOB_KEY"] and r["LNAME_MATCH"] and r["FNAME_MATCH"]:
            return md5("FO", r["DOB_KEY"], r["GENDER_KEY"], r["FNAME_MATCH"], r["LNAME_MATCH"])
        return md5("SINGLE", r["PARTY_ID"])
    if r["CREGNUM_STD"]:
        return md5("ICO", r["CREGNUM_STD"])
    if r["NAME_MATCH"]:
        return md5("PO", r["NAME_MATCH"])
    return md5("SINGLE", r["PARTY_ID"])
```

Three things it rests on:

- **A prefix in the hash** (`'RC'`, `'FO'`, `'ICO'`) prevents collisions between the levels of the cascade.
  Without it two different entities with an identical string can meet.
- **Its own cluster instead of NULL.** A record without a key must get a unique MCODE, otherwise all
  unidentifiable records pour into one false cluster.
- **The `_MATCH` variants go into the key**, not the display values. `Nováková` vs `NOVAKOVA` must give the same
  key.

### `USABLE` vs `VALID`: the most important decision

Separate two different roles of the identifier:

| Role | Criterion | Use |
|---|---|---|
| **dedup signal** (`_KEY`, `USABLE`) | the structure is readable even if the checksum failed | an internal composite key: a readable identifier still reliably joins two records of the same person |
| **published value** (`_STD`, `VALID`) | passed the checksum | export, derived attributes, reporting |

When an unreadable identifier also lacks a derived date, use the original stored date (if credible) as the **key**
fallback; it maximises the dedup signal without anything invalid reaching the output.

This is looser than the academic standard, which lets only checksum-valid identifiers into both matching and
derivation. It is not a contradiction (an invalid value is not published here either), but admit it as your own
choice and show the difference in the number of merges. It is exactly variant A in the key comparison below.

## Where a deterministic key is not enough

The MCODE above is an exact match over unified values. It catches duplicates that match character for character
after standardisation, and misses everything else: a typo in the surname, a swapped date, a missing part of the
composite. In real data that is most of them, so **a deterministic key alone does not finish dedup**.

The second layer is **probabilistic record linkage** (Fellegi–Sunter). Instead of "key equal / not equal" it
computes a likelihood ratio for each pair from two probabilities per compared attribute:

| Probability | Question |
|---|---|
| **m** | how often the attribute matches when it is the **same** entity (typos lower it) |
| **u** | how often it matches **by chance** for two different entities (high for the surname "Novák", low for the date of birth) |

A weighted sum over the attributes gives a match score. Attributes are not compared for equality but by a
similarity measure: Jaro-Winkler or n-grams for names, a distance for dates. In Python there is `splink`, which
estimates m and u from the data instead of you guessing them.

You do not discard the deterministic key: keep it for clean identifiers (company ID, checksum-valid birth number),
where it is fast and certain, and run the probabilistic layer on the rest.

### Blocking

Comparing everyone with everyone is O(n²): for 400 thousand clients, 80 billion pairs. So comparison happens only
inside blocks, and the role of an attribute decides what it is for:

| Attribute role | Example | What for |
|---|---|---|
| **identity** | birth number, company ID, first name, surname | the comparison itself |
| **discriminating** | date of birth, gender | tells father and son at the same address apart |
| **qualifying** | country, entity type, postcode | **blocking key**, splits the data into blocks |

The blocking key must be an attribute duplicates almost always share. Choose it badly and a twin falls into another
block, is never compared, and you cannot tell from the result.

### Threshold and grey zone

A score is not a verdict. Set two thresholds:

- **above the upper one**: merge automatically,
- **below the lower one**: do not merge,
- **between them**: an *uncertain match*: escalate to a **data steward**, do not let the algorithm decide.

Do not shrink the grey zone by pulling the thresholds together. Its size is information about the data, and a queue
for manual review is a legitimate output of deduplication, not a failure.

**A false merge is worse than an unmerged duplicate.** An unmerged duplicate costs money: two campaigns, fragmented
history, wrong CLV. A falsely merged client sees another person's contracts and personal data in the portal; that is
a confidentiality and GDPR incident, not an inaccuracy in a report. Set thresholds in that direction, not by how
many merges they produce.

## Clustering and survivor

Within a cluster pick one survivor record. The score combines quality, activity and recency:

```python
df["_rec"] = pd.to_datetime(df["LAST_ACTIVITY"], errors="coerce").map(
    lambda x: x.toordinal() if pd.notna(x) else 0
)
df["_surv"] = df["DQM_VALID_SCORE"] * 3 + df["TRANSACTION_COUNT"] * 0.5 + df["_rec"] / 1e6
df = df.sort_values(["MCODE", "_surv", "PK"], ascending=[True, False, True])
df["SURV_RECORD_IND"] = (~df.duplicated("MCODE", keep="first")).astype(int)
```

Justify the weights in the report; they are business decisions, not mathematics. A deterministic tiebreak (`PK` as
the last criterion) is mandatory, otherwise the results are not reproducible.

**Do not delete duplicates.** Flag them. Deleting throws away evidence, makes an audit of the merge impossible, and
with a wrong deduplication the loss is irreversible.

Modern MDM goes further than a flag: duplicate records stay in the base **linked by a relationship that carries the
match probability**, and the cluster just has a designated best representative. The difference is practical: for
borderline matches you keep the door open, because a low-probability link can be reassessed later, while a merge is
painful to reassess.

### Record quality score

The share of passed checks by entity type, an input to the survivor score and a metric for the report:

```python
def dqm(r):
    checks = []
    if r["PARTY_TYPE"] == "P":
        checks += [
            r["RC_VALID"],
            r["FNAME_STD"] is not None,
            r["LNAME_STD"] is not None,
            r["DOB_STD"] is not None,
            r["GENDER_STD"] is not None,
        ]
    else:
        checks += [r["CREGNUM_STD"] is not None, r["NAME_STD"] is not None]
    checks.append(any(pd.notna(r.get(c)) for c in ("ADDR_PERM", "ADDR_CORR", "ADDR_OTHER")))
    checks.append(any(pd.notna(r.get(c)) for c in ("CONT_EMAIL", "CONT_MOBILE")))
    return round(sum(1 for c in checks if c) / len(checks), 4)
```

The set of checks **differs by entity type**, otherwise a company never reaches a full score because of fields that
make no sense for it.

## Consolidating the golden record

The survivor need not be the best in every attribute. After selection, propagate the best available variant within
the cluster; that also fixes what the registry in `dq-standardizer` did not:

```python
def coalesce_name(std_col, corrupt_col):
    clean = df[std_col].where(~df[corrupt_col].fillna(False) & df[std_col].notna())
    best = (
        pd.DataFrame({"MCODE": df["MCODE"], "v": clean})
        .dropna(subset=["v"])
        .groupby(["MCODE", "v"])
        .size()
        .reset_index(name="n")
        .sort_values(["MCODE", "n"], ascending=[True, False])
        .drop_duplicates("MCODE")
        .set_index("MCODE")["v"]
    )
    repl = df["MCODE"].map(best)
    use = (df[corrupt_col].fillna(False) | df[std_col].isna()) & repl.notna()
    return df[std_col].where(~use, repl)
```

Selection criterion: an uncorrupted, non-empty variant, the most frequent within the cluster. `BE0ŠOVÁ` is fixed to
`BENEŠOVÁ` if a clean twin of the same person exists in the cluster.

Likewise for dates: an attribute like "client since" is taken as **the minimum over the whole cluster**, not per
row; otherwise merging two records pushes the start of the relationship forward.

```python
df["SINCE_STD"] = (
    df.assign(_s=pd.to_datetime(df["SINCE_STD"], errors="coerce"))
    .groupby("MCODE")["_s"]
    .transform("min")
    .dt.date
)
```

## Household identification

A group of people at the same address with a shared surname root:

```python
def surname_root(match_lname):
    """De-gendered root: the male and female forms → the same root."""
    r = match_lname.split()[0] if " " in match_lname else match_lname
    if r.endswith("OVA") and len(r) > 4:
        r = r[:-3]  # feminine -ová
    elif r.endswith("A") and len(r) > 3:
        r = r[:-1]  # adjectival feminine -á and masculine -a
    elif r.endswith("Y") and len(r) > 3:
        r = r[:-1]  # adjectival masculine -ý
    return r or None


def hshld(r):
    if r["PARTY_TYPE"] != "P":
        return None  # natural persons only
    root = surname_root(r["LNAME_MATCH"])
    code = next(
        (c for c in (r.get("ADDR_CORR"), r.get("ADDR_PERM"), r.get("ADDR_OTHER")) if pd.notna(c)), None
    )
    if not root or code is None:
        return None
    return md5("H", root, int(code))
```

Three rules:

- **Building level only.** A street or municipality code turns a whole village into one household.
- **No address, no household.** NULL is the right output, not a fallback to the surname.
- **Leave out legal entities.** Companies at one address are not a household.

De-gendering is language-specific. For another language replace the rules; without them `Novák` and `Nováková` at
the same address end up in two households.

## Comparing key variants

Before the final decision, compute **both variants** and report the difference:

| Variant | Key | Effect |
|---|---|---|
| A (looser) | any readable identifier | more merges, risk of false positives |
| B (stricter) | only checksum-valid + composite | fewer merges, safer |

```python
print(f"variant A: {va} entities | variant B (used): {df['MCODE'].nunique()} entities")
```

The difference between them is an estimate of the uncertainty of deduplication. Write it in the report together
with which variant you chose and why.

## MDM is a process, not a script

A deduplication run produces a clean state as of one day. For it to last, an architecture has to stand around it:

| Architecture | What it does | Flow back to the sources |
|---|---|---|
| **Autonomy** | MDM inside one application | no |
| **Consolidation** | consolidation in the data warehouse (analytical MDM) | no |
| **Back propagation** | consolidation + writing the golden record back to the sources | yes, in batches |
| **MDM Hub** | a central service providing the unified record in real time | yes, continuously |

A hub holds data in one of three ways: **registry** (only an index and references into the source systems),
**repository** (all attributes physically central), **hybrid** (the most used attributes centrally, the rest stays in
the sources). An alternative to a relational hub is a graph database over the **POLE** model (persons, objects,
locations, events): duplicates are not deleted, just linked by an edge, and traversing relationships is orders of
magnitude faster than a repeated JOIN.

Two terms that get mixed up: the **system of record** is where master data is created and maintained; the **system of
reference** is where correct and current data is read from for reporting. In a hub, both coincide.

Without data owners, stewards and an input control (`dq-guard`), the duplicates come back. Deduplication in the data
warehouse is cleaning, not prevention.

## Output of the deduplicator

A denormalised table, one row per input record, plus:

| Column | Meaning |
|---|---|
| `MCODE` | the entity cluster key |
| `SURV_RECORD_IND` | 1 = survivor (golden record), 0 = duplicate |
| `HSHLD_MCODE` | the household key |
| `DQM_VALID_SCORE` | record quality score 0–1 |

Summary for the report: number of rows, unique entities (MCODE), survivors, duplicates (rows − survivors),
households, match rate to the registry, share of valid identifiers, share of filled contacts.

## Common mistakes

| Mistake | Consequence | Instead |
|---|---|---|
| Deduplicating unstandardised data | a falsely low duplicate count | run after parser, standardiser and imputer |
| MCODE without a level prefix, or NULL for records without a key | collisions between levels; all unidentifiable records in one cluster | prefix per level, `SINGLE` + PK as fallback |
| Display values in the key | `Nováková` and `NOVAKOVA` stay apart | build the key from `_MATCH` variants |
| Narrowing the grey zone to get more merges | false merges, a confidentiality and GDPR incident | escalate uncertain matches to a data steward |
| Deleting duplicates | evidence lost, a wrong merge cannot be undone | flag them (`SURV_RECORD_IND`) or link them with a match probability |
| A survivor score without a deterministic tiebreak | results differ between runs | `PK` as the last sort key |

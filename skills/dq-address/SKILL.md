---
name: dq-address
description: Links addresses to an address registry (RÚIAN, UIR-ADR or any national address registry). Builds a match code, a hierarchy of indexes by precision (building, street, municipality), fuzzy matching without diacritics, measures the match rate, handles collation and scope gaps of the registry (P.O. boxes), and assigns the address point code and a deliverability flag. Remediation step between dq-standardizer and dq-imputer. Use it whenever addresses need to be "linked to the registry", "matched to RÚIAN", "geocoded", "given an address code", "checked for deliverability", "counted as valid", even if the user does not name the registry. Keywords: RÚIAN, UIR-ADR, address registry, data enrichment, match code, address code, geocoding, address matching, deliverability, match rate.
---

# Address: linking to the address registry

An address without a registry code is just text. With a code it is a verified delivery address you can hang
geocoding, location-based pricing and household identification on. In insurance, geocoding is a direct input
to the rate (flood zone, theft risk, distance from the fire station), so an unlinked address is not only
undeliverable but also mispriced.

Methodologically this step is **data enrichment**: linking to an external authoritative source and taking a
value your own data does not carry. The address registry is just the most common case; the same applies to a
company registry or an industry classification.

Pipeline: `dq-standardizer` → **dq-address** → `dq-imputer` → `dq-deduplicator`. Precondition: the address is
already atomised (`dq-parser`) and the postal code standardised.

The examples are from the Czech RÚIAN/UIR-ADR. The procedure is the same for any national address registry
(USPS, Ordnance Survey, a national address registry); only the column names and the postcode format change.
The principle "match code + precision hierarchy + no fallback" holds everywhere.

## First check whether the registry can carry the role of a standard

Before matching, profile **the registry itself**. Real findings from the audited UIR-ADR:

| Registry problem | Extent | Consequence |
|---|---|---|
| registry abolished and replaced | UIR-ADR dead since 2012, successor RÚIAN | validating against it is meaningless in principle |
| missing coordinates | 2,305,082 of 2,597,960 rows (88.7 %) without X and Y | registry unusable for geocoding |
| inverted sign of coordinates | 292,878 records with X ≥ 0 or Y ≥ 0 | nonsense in the S-JTSK convention |
| missing PK and indexes | the whole table | a JOIN over 2.6 M rows without an index = unbearable |
| incomplete sister table | 49,670 valid addresses (1.91 %) missing from the merged version | silent losses in matching |
| broken atomicity in the registry | 21,145 rows have a digit in the street field | the registry has the same defect as operations |
| empty string instead of NULL | `cisor_pis` in 99.46 % of rows | `COALESCE` does not work, `NULLIF(TRIM(x),'')` needed |
| mixed collation | latin1 vs utf8 | the JOIN fails or silently does not match |
| **scope gap** | the registry contains no P.O. boxes by design | valid addresses will always be "invalid" |

The last row deserves its own paragraph in the report. An address with a post office box is formally fine; that
the registry does not know it is a limit of the source, not a data defect. Without saying so you report 100 % of
those rows as errors.

## Match code: the principle

A match code is a hash of the normalised address string, computed **by the same recipe** on both sides. It
allows a JOIN on one indexed column instead of comparing five text fields.

```sql
ALTER TABLE PARTY_ADDRESS       ADD ADDR_CODE bigint, ADD ADDR_FMCODE char(32);
ALTER TABLE REF_UIRADR_MERGED   ADD MCODE_FUZZY char(32);
CREATE INDEX ix_addr_fmcode ON PARTY_ADDRESS (ADDR_FMCODE);
CREATE INDEX ix_ref_mcode   ON REF_UIRADR_MERGED (MCODE_FUZZY);
```

The normalisation recipe (must be identical on both sides): remove diacritics, lowercase, remove everything
except letters and digits, join the parts with a fixed separator.

```sql
-- registry side
UPDATE REF_UIRADR_MERGED SET MCODE_FUZZY = MD5(replaceDiacritics(LOWER(TRIM(CONCAT(
  COALESCE(psc,''),'_', COALESCE(TRIM(ulice),''), COALESCE(cisdom_hod,''),
  CASE WHEN NULLIF(TRIM(cisor_hod),'') IS NOT NULL THEN CONCAT('/',cisor_hod) ELSE '' END,
  '_', COALESCE(TRIM(obec),''),
  CASE WHEN mcast IS NOT NULL AND TRIM(obec) <> '' THEN CONCAT(' - ',TRIM(mcast))
       WHEN mcast IS NOT NULL                      THEN TRIM(mcast) ELSE '' END )))));

-- operations side
UPDATE PARTY_ADDRESS SET ADDR_FMCODE = MD5(replaceDiacritics(LOWER(TRIM(CONCAT(
  COALESCE(ADDR_ZIP_STD,''),'_', COALESCE(TRIM(ADDR_STREET_STD),''),'_',
  COALESCE(TRIM(ADDR_CITY_STD),'') )))));

UPDATE PARTY_ADDRESS a INNER JOIN REF_UIRADR_MERGED b ON b.MCODE_FUZZY = a.ADDR_FMCODE
SET a.ADDR_CODE = b.adresa_kod;
```

When the JOIN on the hash returns nothing, it is typically the **collation** of the two hash columns, not the
logic. Unify them explicitly and run it again:

```sql
ALTER TABLE REF_UIRADR_MERGED MODIFY MCODE_FUZZY char(32)
  CHARACTER SET utf8 COLLATE utf8_general_ci NULL;
```

## Index hierarchy, and why you must not fall back

An address registry is not a flat list of addresses but a hierarchy of territorial elements: address point,
street, building, cadastral area, municipality, district, region. The level you hit is the level you can
aggregate to. So build several indexes by precision:

| Index | Key | Precision |
|---|---|---|
| `idx_bld` | postcode + municipality + street + building number | a specific building |
| `idx_bldv` | postcode + municipality + building number (municipalities without streets) | a specific building in a village |
| `idx_str` | postcode + municipality + street | street only |
| `idx_zc` | postcode + municipality | municipality only |

```python
def match_code(zip5, city_n, street_n, num):
    if not zip5 or not city_n or not num:
        return None
    if street_n:
        return idx_bld.get((zip5, city_n, street_n, num))  # exact building, otherwise None
    return idx_bldv.get((zip5, city_n, num))  # village without streets
```

**No fallback inside one column.** A street or municipality code written into the building field looks like a
successful link but points to *another building*. A silently wrong code is worse than a missing one; it spreads
into delivery, geocoding and household identification.

That does not mean discarding the coarser match. It means not passing it off as precise: store each level in
its own column and add a flag of the precision achieved.

| Column | Content |
|---|---|
| `ADDR_CODE` | address point code (building), otherwise NULL |
| `ADDR_CODE_STR` / `ADDR_CODE_MUN` | street / municipality code when the building did not match |
| `ADDR_MATCH_LEVEL` | `BLD` / `STR` / `MUN` / `NONE` |

The consumer then picks the precision it needs. Geocoding, pricing by risk zone, delivery and household need
the **building**. Aggregated reporting (to the regulator by postcode or district), a regional campaign or a
blanket mailing can do with the street or municipality; forcing NULL there would ruin the report.

The report states both: "63 % to building level, 89 % to street level."

## Fuzzy matching: what helps and what does not

| Technique | Effect |
|---|---|
| removing diacritics on both sides | the single biggest jump in match rate |
| lowercase + removing punctuation and spaces | essential, practically free |
| `COALESCE(obec, mcast, cobce)` on the registry side | covers rows with an empty municipality (~51 % in UIR-ADR) |
| the first segment before the dash in the municipality field | resolves `Praha 5 - Smíchov` |
| trimming the postcode to 5 digits + excluding placeholders (`99999`, `00000`) | removes false matches |
| **phonetic algorithms** (Soundex, Metaphone) | a standard unification tool, but language-bound: both classic variants are tuned for English and produce false matches in Czech. Use a variant for the target language, or leave them out and measure how many matches you lose |

Raise the match rate **iteratively and measure after each step**. A jump of tens of percent after one change is
suspicious; check a sample to see whether you are matching nonsense.

## Metrics for the report

```sql
SELECT ROUND(100.0*SUM(ADDR_CODE IS NOT NULL)/COUNT(*),2) match_rate_pct FROM PARTY_ADDRESS;
```

Report three numbers separately: the match rate to building level (the only one used), the share of addresses
excluded because of a missing component (postcode / municipality / number) and the share outside the registry's
scope (P.O. box). The sum must be 100 %; otherwise something disappears silently.

The starting state in the audited database was **0 % exact match** on the triple postcode + municipality +
street, because house numbers were merged into the street and the municipality held the whole hierarchy. After
parsing and standardisation, matching took off. That is the most telling before/after in the whole remediation;
use it.

## Pivot by address type

For the golden record you need one code per client and type. Take the first non-null code of the given type:

```python
def pivot_type(t):
    sub = addr[(addr["ADDR_TYPE"] == t) & addr["ADDR_CODE"].notna()]
    return sub.groupby("PARTY_ID")["ADDR_CODE"].first()


addr_piv = pd.DataFrame(
    {"ADDR_PERM": pivot_type("R"), "ADDR_CORR": pivot_type("C"), "ADDR_OTHER": pivot_type("O")}
).reset_index()
```

Watch for an address type outside the code list (code `P` on 40 % of addresses in the audited database). Decide
**before** the pivot whether to remap it to an existing code or add it to the code list; otherwise 40 % of
addresses drop out of the pivot and the golden record has no permanent address.

## Downstream use

- `dq-imputer`: looks up a missing postcode or municipality from the address code.
- `dq-deduplicator`: the address code is part of the household key.
- Deliverability flag = the address has a building-level code and is not a P.O. box.

## Common mistakes

| Mistake | Consequence | Instead |
|---|---|---|
| Matching before profiling the registry | validation against an abolished or incomplete source | check coverage, coordinates, keys and scope gaps first |
| A different normalisation recipe on each side | the match code never meets | one recipe for both sides; when the hash join returns nothing, check collation |
| A street or municipality code in the building field | the address points to another building | one column per level plus `ADDR_MATCH_LEVEL` |
| Reporting P.O. boxes as unmatched errors | a limit of the source is reported as bad data | report them as out of the registry's scope |
| Raising the match rate several steps at once | a jump that may be matching nonsense | one change at a time, measure and check a sample |
| Pivoting before deciding unknown address types | a large share of addresses drops out of the golden record | remap or extend the code list before the pivot |

# Check catalogue (SQL)

MySQL syntax. PostgreSQL: `REGEXP` → `~`, `IFNULL` → `COALESCE`, `TIMESTAMPDIFF(YEAR,a,b)` →
`EXTRACT(YEAR FROM age(b,a))`, drop `CONVERT(x USING utf8mb4)`, `SUM(bool)` →
`COUNT(*) FILTER (WHERE ...)`.

---

## 1. Completeness (COMPLETENESS)

```sql
-- three levels of emptiness separately: NULL, empty string, placeholder
SELECT COUNT(*) universe,
  SUM(<col> IS NULL)                                          as_null,
  SUM(<col> IS NOT NULL AND TRIM(<col>) = '')                 as_empty,
  SUM(TRIM(COALESCE(<col>,'')) IN
      ('NA','N/A','NULL','.','-','cizinec','NEVYPLNENO'))     as_placeholder
FROM <t> WHERE <universe>;
```

Completeness per column in one query (UNION ALL because the universes differ):

```sql
SELECT 'PARTY_RC' col, SUM(PARTY_RC IS NULL OR TRIM(PARTY_RC)='') missing, COUNT(*) total
  FROM PART_PARTY WHERE PARTY_TYPE='P'
UNION ALL SELECT 'PARTY_GENDER', SUM(PARTY_GENDER IS NULL OR TRIM(PARTY_GENDER)=''), COUNT(*)
  FROM PART_PARTY WHERE PARTY_TYPE='P'
UNION ALL SELECT 'PARTY_CREGNUM', SUM(PARTY_CREGNUM IS NULL OR TRIM(PARTY_CREGNUM)=''), COUNT(*)
  FROM PART_PARTY WHERE PARTY_TYPE='C';
```

Across tables: an entity without a mandatory relationship (completeness, not integrity):

```sql
SELECT COUNT(*) clients_without_address FROM PART_PARTY p
LEFT JOIN PARTY_ADDRESS a ON p.PARTY_ID = a.PARTY_ID WHERE a.PARTY_ID IS NULL;

SELECT COUNT(*) companies_without_contact FROM PART_PARTY p
WHERE p.PARTY_TYPE='C' AND NOT EXISTS (SELECT 1 FROM PARTY_CONTACT c WHERE c.PARTY_ID=p.PARTY_ID);
```

A column never filled (100 % NULL) is a finding category of its own: either drop it, or fill it with a
`NOT NULL` strategy. Do not leave it "unfinished".

---

## 2. Syntactic correctness (SYN_CORR)

```sql
SELECT COUNT(*) total,
  SUM(<col> <> TRIM(<col>))                              whitespace,
  SUM(LENGTH(<col>) <> LENGTH(TRIM(<col>)))              padding,
  SUM(BINARY TRIM(<col>) <> BINARY UPPER(TRIM(<col>)))   not_upper,
  SUM(NOT (<col> REGEXP '<pattern>'))                    bad_format
FROM <t> WHERE <col> IS NOT NULL AND TRIM(<col>) <> '';
```

### Birth number (Czech national ID)

```sql
SELECT SUM(PARTY_TYPE='P' AND PARTY_RC IS NOT NULL
       AND NOT (REPLACE(PARTY_RC,'/','') REGEXP '^[0-9]{9,10}$')) rc_bad_format,
       SUM(PARTY_RC REGEXP '[A-Za-z]')                            rc_text
FROM PART_PARTY;

-- checksum mod 11 (10-digit ones only)
SELECT COUNT(*) rc_bad_checksum FROM PART_PARTY
WHERE PARTY_TYPE='P' AND REPLACE(PARTY_RC,'/','') REGEXP '^[0-9]{10}$'
  AND CAST(REPLACE(PARTY_RC,'/','') AS UNSIGNED) % 11 <> 0
  AND NOT (CAST(LEFT(REPLACE(PARTY_RC,'/',''),9) AS UNSIGNED) % 11 = 10
           AND RIGHT(REPLACE(PARTY_RC,'/',''),1) = '0');

-- impossible month (blocks the cross-checks that follow)
SELECT COUNT(*) rc_month_out_of_range FROM PART_PARTY
WHERE PARTY_TYPE='P' AND PARTY_RC REGEXP '^[0-9]{9,10}$'
  AND (CAST(SUBSTR(PARTY_RC,3,2) AS UNSIGNED) % 50) NOT BETWEEN 1 AND 12;
```

### Company ID (IČO)

```sql
SELECT SUM(PARTY_TYPE='C' AND PARTY_CREGNUM IS NOT NULL
       AND NOT (PARTY_CREGNUM REGEXP '^[0-9]{8}$')) ico_bad_format
FROM PART_PARTY;
-- checksum mod 11: weights 8..2 on the first 7 digits; check digit = (11 - sum%11) % 10
```

### E-mail: tell format apart from a systematic swap

```sql
SELECT COUNT(*) emails,
  SUM(NOT (TRIM(CONT_VALUE) REGEXP
      '^[A-Za-z0-9._+%-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,24}$'))  invalid,
  SUM(CONT_VALUE LIKE '%#%')     hash_instead_of_at,
  SUM(CONT_VALUE LIKE '%&%')     amp_instead_of_dot,
  SUM(CONT_VALUE LIKE '% %')     inner_space,
  SUM(CONT_VALUE NOT LIKE '%@%') no_at
FROM PARTY_CONTACT WHERE CONT_TYPE='E' AND TRIM(COALESCE(CONT_VALUE,'')) <> '';
```

The difference is fundamental: "22,694 invalid e-mails" is a data defect; "22,694 e-mails with `#` instead of
`@` and `&` instead of `.`" is **evidence of a systematic import error** with a clear remedy (a mapping
replace, not manual cleaning).

### Phone

```sql
SELECT CASE WHEN TRIM(CONT_VALUE) REGEXP '^[0-9]{9}$'       THEN '9_digits_OK'
            WHEN TRIM(CONT_VALUE) REGEXP '^\\+420[0-9]{9}$' THEN 'with_prefix'
            WHEN TRIM(COALESCE(CONT_VALUE,'')) = ''         THEN 'empty'
            ELSE 'INVALID' END flag, COUNT(*) freq
FROM PARTY_CONTACT WHERE CONT_TYPE='M' GROUP BY 1 ORDER BY freq DESC;

-- landline recorded as mobile (semantics, not syntax)
SELECT COUNT(*) landline_as_mobile FROM PARTY_CONTACT
WHERE CONT_TYPE='M' AND TRIM(CONT_VALUE) REGEXP '^[2-5][0-9]{8}$';
```

### Postcode

```sql
SELECT CASE WHEN ADDR_ZIP REGEXP '^[0-9]{5}$' THEN 'OK'
            WHEN TRIM(COALESCE(ADDR_ZIP,'')) = '' THEN 'NULL_or_empty'
            ELSE 'BAD' END flag, COUNT(*) freq
FROM PARTY_ADDRESS WHERE ADDR_COUNTRY='CZE' GROUP BY 1;

-- a sample of BAD shows the root cause (char(5) truncation after a space): '251 6', '400 0'
SELECT ADDR_ZIP, COUNT(*) freq FROM PARTY_ADDRESS
WHERE ADDR_COUNTRY='CZE' AND (ADDR_ZIP NOT REGEXP '^[0-9]{5}$' OR ADDR_ZIP IS NULL)
GROUP BY 1 ORDER BY freq DESC LIMIT 15;
```

### Names and titles

```sql
-- interleaved spaces: 'D A V I D'
SELECT COUNT(*) FROM PART_PARTY WHERE PARTY_FNAME REGEXP '^([A-Za-zÀ-ž] )+[A-Za-zÀ-ž]$';

-- encoding error: digits inside a name, a foreign character at the start
SELECT PARTY_LNAME, COUNT(*) FROM PART_PARTY
WHERE PARTY_LNAME REGEXP '[0-9]' OR PARTY_LNAME LIKE 'Ą%' GROUP BY 1 ORDER BY 2 DESC;

-- padding from char(n) + spelling variants of the title
SELECT LENGTH(PARTY_TITBEF) len, LENGTH(TRIM(PARTY_TITBEF)) len_trim,
       PARTY_TITBEF, COUNT(*) freq
FROM PART_PARTY WHERE TRIM(COALESCE(PARTY_TITBEF,'')) <> ''
GROUP BY 1,2,3 ORDER BY freq DESC LIMIT 20;

-- match against the reference dictionary: exact vs. after normalisation
SELECT COUNT(*) rows_with_title,
  SUM(EXISTS (SELECT 1 FROM REF_TITBEF r
       WHERE CONVERT(TRIM(r.VALUE) USING utf8mb4) = CONVERT(TRIM(p.PARTY_TITBEF) USING utf8mb4)))
    AS exact_match,
  SUM(EXISTS (SELECT 1 FROM REF_TITBEF r
       WHERE LOWER(REPLACE(CONVERT(TRIM(r.VALUE) USING utf8mb4),'.','')) =
             LOWER(REPLACE(CONVERT(TRIM(p.PARTY_TITBEF) USING utf8mb4),'.',''))))
    AS match_after_normalisation
FROM PART_PARTY p WHERE TRIM(COALESCE(p.PARTY_TITBEF,'')) <> '';
```

A real ratio: exact 21–29 %, after normalisation ~81 %. That difference is exactly the amount of
standardisation work and the strongest argument for the corrective action.

---

## 3. Semantic correctness (SEM_CORR)

```sql
-- outside the code list (anti-join + collation-safe)
SELECT COUNT(*) outside_lov FROM <t> a
LEFT JOIN <lov> l ON CONVERT(a.<col> USING utf8mb4) = CONVERT(l.CODE USING utf8mb4)
WHERE a.<col> IS NOT NULL AND l.CODE IS NULL;

-- ranges
SELECT SUM(PARTY_AGE < 0) negative, SUM(PARTY_AGE > 130) impossible FROM PART_PARTY;

-- sentinel date
SELECT COUNT(*) sentinel, ROUND(100.0*COUNT(*)/(SELECT COUNT(*) FROM PROD_CONTRACT),2) pct
FROM PROD_CONTRACT WHERE CNTR_VALIDTO >= '2999-01-01';

-- blanket constant (pct = 100 → finding)
SELECT <col>, COUNT(*) freq, ROUND(100.0*COUNT(*)/(SELECT COUNT(*) FROM <t>),2) pct
FROM <t> GROUP BY 1 ORDER BY freq DESC LIMIT 5;

-- a value that belongs in another column (company ID in the sector field)
SELECT COUNT(*) FROM PART_PARTY WHERE CAST(PARTY_ESA95 AS CHAR) = TRIM(PARTY_CREGNUM);
```

What a regex will not catch: a composite value in an atomic field (`Praha 5 - Smíchov`), the whole address
hierarchy in the municipality field, a role instead of a title (`otec`, father), a landline under the mobile
type, a retired product (`DEL_FLAG=1`) still in the catalogue with a valid `VALID_TO`.

---

## 4. Internal consistency (INT_CONS)

```sql
-- derived attribute vs. source (stored age vs. date of birth)
SELECT SUM(ABS(PARTY_AGE - TIMESTAMPDIFF(YEAR, PARTY_DOFBIRTH, '<snapshot>')) > 1) mismatch,
       MAX(ABS(PARTY_AGE - TIMESTAMPDIFF(YEAR, PARTY_DOFBIRTH, '<snapshot>'))) max_difference
FROM PART_PARTY WHERE PARTY_TYPE='P' AND PARTY_AGE IS NOT NULL AND PARTY_DOFBIRTH IS NOT NULL;

-- identifier vs. derived attributes (birth number ↔ date of birth ↔ gender)
SELECT COUNT(*) FROM PART_PARTY
WHERE PARTY_TYPE='P' AND PARTY_RC REGEXP '^[0-9]{9,10}$'
  AND PARTY_RC NOT IN ('9999999999','0000000000') AND PARTY_DOFBIRTH IS NOT NULL
  AND ( SUBSTRING(PARTY_RC,1,2) <> RIGHT(CAST(YEAR(PARTY_DOFBIRTH) AS CHAR),2)
     OR (PARTY_GENDER='M' AND CAST(SUBSTRING(PARTY_RC,3,2) AS UNSIGNED) <> MONTH(PARTY_DOFBIRTH))
     OR (PARTY_GENDER='F' AND CAST(SUBSTRING(PARTY_RC,3,2) AS UNSIGNED) <> MONTH(PARTY_DOFBIRTH)+50) );

-- mutually exclusive attributes filled at the same time
SELECT SUM(PARTY_TYPE='P' AND TRIM(COALESCE(PARTY_CREGNUM,'')) <> '') person_with_company_id,
       SUM(PARTY_TYPE='C' AND TRIM(COALESCE(PARTY_RC,''))      <> '') company_with_birth_number,
       SUM(PARTY_TYPE='C' AND TRIM(COALESCE(PARTY_FNAME,'')) NOT IN ('','NA')) company_with_first_name
FROM PART_PARTY;

-- paired attributes: one filled, the other not
SELECT SUM(TRIM(COALESCE(ADDR_CITY,''))<>''   AND TRIM(COALESCE(ADDR_ZIP,''))='')  city_without_zip,
       SUM(TRIM(COALESCE(ADDR_ZIP,''))<>''    AND TRIM(COALESCE(ADDR_CITY,''))='') zip_without_city,
       SUM(TRIM(COALESCE(ADDR_STREET,''))<>'' AND TRIM(COALESCE(ADDR_NUM1,''))=''
                                              AND TRIM(COALESCE(ADDR_NUM2,''))='') street_without_number
FROM PARTY_ADDRESS;

-- time ranges + forensic detail
SELECT COUNT(*) reversed_range,
       MIN(DATEDIFF(CNTR_VALIDFROM,CNTR_VALIDTO)) min_days,
       MAX(DATEDIFF(CNTR_VALIDFROM,CNTR_VALIDTO)) max_days,
       ROUND(AVG(DATEDIFF(CNTR_VALIDFROM,CNTR_VALIDTO)),1) avg_days
FROM PROD_CONTRACT WHERE CNTR_VALIDFROM > CNTR_VALIDTO;

SELECT YEAR(CNTR_VALIDFROM) year, COUNT(*) FROM PROD_CONTRACT
WHERE CNTR_VALIDFROM > CNTR_VALIDTO GROUP BY 1 ORDER BY 2 DESC;   -- all from one year = a batch

-- status vs. date: cancelled, but valid "forever"
SELECT COUNT(*) cancelled_with_infinity FROM PROD_CONTRACT
WHERE CNTR_CANCTYPE IS NOT NULL AND CNTR_VALIDTO >= '2999-01-01';

-- contract outside the product's validity (cross-check against the catalogue)
SELECT SUM(c.CNTR_VALIDFROM < p.VALID_FROM) before_launch,
       SUM(c.CNTR_VALIDFROM > p.VALID_TO)   after_retirement
FROM PROD_CONTRACT c JOIN PRODUCT_CATALOGUE p ON c.PRODUCT_CODE = p.PRODUCT_CODE;
```

---

## 5. External consistency (EXT_CONS)

```sql
-- orphans
SELECT COUNT(*) orphans FROM PARTY_CONTACT c
LEFT JOIN PART_PARTY p ON c.PARTY_ID = p.PARTY_ID WHERE p.PARTY_ID IS NULL;

-- forensic follow-up: do the orphan IDs form a contiguous range after the last parent ID?
SELECT MIN(c.PARTY_ID) id_from, MAX(c.PARTY_ID) id_to,
       COUNT(DISTINCT c.PARTY_ID) distinct_ids,
       MAX(c.PARTY_ID)-MIN(c.PARTY_ID)+1 range_size,
       (SELECT MAX(PARTY_ID) FROM PART_PARTY) last_parent
FROM PARTY_CONTACT c LEFT JOIN PART_PARTY p ON c.PARTY_ID = p.PARTY_ID
WHERE p.PARTY_ID IS NULL;
-- distinct_ids = range_size and id_from = last_parent+1 → unfinished import from another system
```

This query turned "37,879 orphans" into a finding with a proven cause: a contiguous range
383,132–421,010 starting right after the last client. Without it, it is just a count.

```sql
-- match rate against the reference registry (a metric, not just an error count)
SELECT ROUND(100.0*SUM(ADDR_CODE IS NOT NULL)/COUNT(*),2) match_rate_pct FROM PARTY_ADDRESS;

-- coverage of the reference table (can it carry the role of a standard?)
SELECT COUNT(*) operational, SUM(r.ICO IS NOT NULL) in_registry,
       ROUND(100.0*SUM(r.ICO IS NOT NULL)/COUNT(*),2) coverage_pct
FROM PART_PARTY p LEFT JOIN REF_PARTY_CREGNUM r ON TRIM(p.PARTY_CREGNUM) = r.ICO
WHERE p.PARTY_TYPE='C';
```

---

## 6. Uniqueness (UNIQUENESS)

```sql
-- duplicates by natural key
SELECT REPLACE(PARTY_RC,'/','') k, COUNT(*) cnt FROM PART_PARTY
WHERE PARTY_TYPE='P' AND TRIM(COALESCE(PARTY_RC,'')) <> ''
GROUP BY 1 HAVING cnt > 1 ORDER BY cnt DESC LIMIT 20;

-- how many groups in total (for the metric), not just the TOP 20
SELECT COUNT(*) dup_groups, SUM(cnt) affected_rows FROM (
  SELECT PARTY_CREGNUM, COUNT(*) cnt FROM PART_PARTY
  WHERE PARTY_TYPE='C' AND TRIM(COALESCE(PARTY_CREGNUM,'')) <> ''
  GROUP BY 1 HAVING COUNT(*) > 1) z;

-- redundancy of the whole record
SELECT COUNT(*) total,
  COUNT(DISTINCT CONCAT_WS('|',IFNULL(ADDR_STREET,''),IFNULL(ADDR_CITY,''),IFNULL(ADDR_ZIP,''))) uniq,
  ROUND(100.0*(COUNT(*) - COUNT(DISTINCT CONCAT_WS('|',IFNULL(ADDR_STREET,''),
        IFNULL(ADDR_CITY,''),IFNULL(ADDR_ZIP,''))))/COUNT(*),2) redundancy_pct
FROM PARTY_ADDRESS;

-- dummy values: one value shared by many entities
SELECT CONT_VALUE, CONT_TYPE, COUNT(DISTINCT PARTY_ID) clients FROM PARTY_CONTACT
GROUP BY 1,2 HAVING clients > 5 ORDER BY clients DESC LIMIT 20;

-- a duplicate combination a unique index should guard (and does not)
SELECT PARTY_ID, CONT_TYPE, COUNT(*) cnt FROM PARTY_CONTACT
GROUP BY 1,2 HAVING cnt > 1 ORDER BY cnt DESC LIMIT 15;
```

**Do not measure duplicates before standardisation**: `MuDr` vs `MUDr.`, `Nováková` vs `NOVAKOVA` are not
caught as duplicates. The measured number is a lower bound; say so in the report.

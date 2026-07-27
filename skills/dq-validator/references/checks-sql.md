# Katalog kontrol (SQL)

MySQL syntax. Postgres: `REGEXP` → `~`, `IFNULL` → `COALESCE`, `TIMESTAMPDIFF(YEAR,a,b)` →
`EXTRACT(YEAR FROM age(b,a))`, `CONVERT(x USING utf8mb4)` vypustit, `SUM(bool)` →
`COUNT(*) FILTER (WHERE ...)`.

---

## 1. Úplnost (COMPLETENESS)

```sql
-- tři úrovně prázdnoty zvlášť: NULL, prázdný řetězec, zástupná hodnota
SELECT COUNT(*) univerzum,
  SUM(<col> IS NULL)                                          jako_null,
  SUM(<col> IS NOT NULL AND TRIM(<col>) = '')                 jako_prazdny,
  SUM(TRIM(COALESCE(<col>,'')) IN
      ('NA','N/A','NULL','.','-','cizinec','NEVYPLNENO'))     jako_placeholder
FROM <t> WHERE <univerzum>;
```

Úplnost po sloupcích jedním dotazem (UNION ALL kvůli různým univerzům):

```sql
SELECT 'PARTY_RC' col, SUM(PARTY_RC IS NULL OR TRIM(PARTY_RC)='') missing, COUNT(*) total
  FROM PART_PARTY WHERE PARTY_TYPE='P'
UNION ALL SELECT 'PARTY_GENDER', SUM(PARTY_GENDER IS NULL OR TRIM(PARTY_GENDER)=''), COUNT(*)
  FROM PART_PARTY WHERE PARTY_TYPE='P'
UNION ALL SELECT 'PARTY_CREGNUM', SUM(PARTY_CREGNUM IS NULL OR TRIM(PARTY_CREGNUM)=''), COUNT(*)
  FROM PART_PARTY WHERE PARTY_TYPE='C';
```

Kříž mezi tabulkami — entita bez povinné vazby (úplnost, ne integrita):

```sql
SELECT COUNT(*) klientu_bez_adresy FROM PART_PARTY p
LEFT JOIN PARTY_ADDRESS a ON p.PARTY_ID = a.PARTY_ID WHERE a.PARTY_ID IS NULL;

SELECT COUNT(*) firem_bez_kontaktu FROM PART_PARTY p
WHERE p.PARTY_TYPE='C' AND NOT EXISTS (SELECT 1 FROM PARTY_CONTACT c WHERE c.PARTY_ID=p.PARTY_ID);
```

Sloupec nikdy nenaplněný (100 % NULL) je vlastní kategorie nálezu — buď odstranit, nebo doplnit
s `NOT NULL` strategií. Nenechávat „nedokončený".

---

## 2. Syntaktická správnost (SYN_CORR)

```sql
SELECT COUNT(*) total,
  SUM(<col> <> TRIM(<col>))                              whitespace,
  SUM(LENGTH(<col>) <> LENGTH(TRIM(<col>)))              padding,
  SUM(BINARY TRIM(<col>) <> BINARY UPPER(TRIM(<col>)))   ne_upper,
  SUM(NOT (<col> REGEXP '<pattern>'))                    bad_format
FROM <t> WHERE <col> IS NOT NULL AND TRIM(<col>) <> '';
```

### Rodné číslo

```sql
SELECT SUM(PARTY_TYPE='P' AND PARTY_RC IS NOT NULL
       AND NOT (REPLACE(PARTY_RC,'/','') REGEXP '^[0-9]{9,10}$')) rc_bad_format,
       SUM(PARTY_RC REGEXP '[A-Za-z]')                            rc_text
FROM PART_PARTY;

-- checksum mod 11 (jen 10místná)
SELECT COUNT(*) rc_bad_checksum FROM PART_PARTY
WHERE PARTY_TYPE='P' AND REPLACE(PARTY_RC,'/','') REGEXP '^[0-9]{10}$'
  AND CAST(REPLACE(PARTY_RC,'/','') AS UNSIGNED) % 11 <> 0
  AND NOT (CAST(LEFT(REPLACE(PARTY_RC,'/',''),9) AS UNSIGNED) % 11 = 10
           AND RIGHT(REPLACE(PARTY_RC,'/',''),1) = '0');

-- nesmyslný měsíc (blokuje navazující křížové kontroly)
SELECT COUNT(*) rc_mesic_mimo FROM PART_PARTY
WHERE PARTY_TYPE='P' AND PARTY_RC REGEXP '^[0-9]{9,10}$'
  AND (CAST(SUBSTR(PARTY_RC,3,2) AS UNSIGNED) % 50) NOT BETWEEN 1 AND 12;
```

### IČO

```sql
SELECT SUM(PARTY_TYPE='C' AND PARTY_CREGNUM IS NOT NULL
       AND NOT (PARTY_CREGNUM REGEXP '^[0-9]{8}$')) ico_bad_format
FROM PART_PARTY;
-- checksum mod 11: váhy 8..2 na prvních 7 číslic; kontrolní = (11 - suma%11) % 10
```

### E-mail — rozliš formát od systematické záměny

```sql
SELECT COUNT(*) emailu,
  SUM(NOT (TRIM(CONT_VALUE) REGEXP
      '^[A-Za-z0-9._+%-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,24}$'))  nevalidnich,
  SUM(CONT_VALUE LIKE '%#%')     hash_misto_zavinace,
  SUM(CONT_VALUE LIKE '%&%')     amp_misto_tecky,
  SUM(CONT_VALUE LIKE '% %')     mezera_uvnitr,
  SUM(CONT_VALUE NOT LIKE '%@%') bez_zavinace
FROM PARTY_CONTACT WHERE CONT_TYPE='E' AND TRIM(COALESCE(CONT_VALUE,'')) <> '';
```

Rozdíl je zásadní: „22 694 nevalidních e-mailů" je defekt dat; „22 694 e-mailů s `#` místo `@`
a `&` místo `.`" je **důkaz systematické chyby importu** s jasným opatřením (mapovací replace,
ne ruční čištění).

### Telefon

```sql
SELECT CASE WHEN TRIM(CONT_VALUE) REGEXP '^[0-9]{9}$'       THEN '9_cifer_OK'
            WHEN TRIM(CONT_VALUE) REGEXP '^\\+420[0-9]{9}$' THEN 's_predvolbou'
            WHEN TRIM(COALESCE(CONT_VALUE,'')) = ''         THEN 'prazdne'
            ELSE 'INVALID' END flag, COUNT(*) freq
FROM PARTY_CONTACT WHERE CONT_TYPE='M' GROUP BY 1 ORDER BY freq DESC;

-- pevná linka vedená jako mobil (sémantika, ne syntax)
SELECT COUNT(*) pevne_jako_mobil FROM PARTY_CONTACT
WHERE CONT_TYPE='M' AND TRIM(CONT_VALUE) REGEXP '^[2-5][0-9]{8}$';
```

### PSČ

```sql
SELECT CASE WHEN ADDR_ZIP REGEXP '^[0-9]{5}$' THEN 'OK'
            WHEN TRIM(COALESCE(ADDR_ZIP,'')) = '' THEN 'NULL_nebo_prazdne'
            ELSE 'BAD' END flag, COUNT(*) freq
FROM PARTY_ADDRESS WHERE ADDR_COUNTRY='CZE' GROUP BY 1;

-- vzorek BAD ukáže root cause (useknutí char(5) po mezeře): '251 6', '400 0'
SELECT ADDR_ZIP, COUNT(*) freq FROM PARTY_ADDRESS
WHERE ADDR_COUNTRY='CZE' AND (ADDR_ZIP NOT REGEXP '^[0-9]{5}$' OR ADDR_ZIP IS NULL)
GROUP BY 1 ORDER BY freq DESC LIMIT 15;
```

### Jména a tituly

```sql
-- prokládané mezery: 'D A V I D'
SELECT COUNT(*) FROM PART_PARTY WHERE PARTY_FNAME REGEXP '^([A-Za-zÀ-ž] )+[A-Za-zÀ-ž]$';

-- kódovací chyba: číslice uvnitř jména, cizí znak na začátku
SELECT PARTY_LNAME, COUNT(*) FROM PART_PARTY
WHERE PARTY_LNAME REGEXP '[0-9]' OR PARTY_LNAME LIKE 'Ą%' GROUP BY 1 ORDER BY 2 DESC;

-- padding z char(n) + varianty zápisu titulu
SELECT LENGTH(PARTY_TITBEF) len, LENGTH(TRIM(PARTY_TITBEF)) len_trim,
       PARTY_TITBEF, COUNT(*) freq
FROM PART_PARTY WHERE TRIM(COALESCE(PARTY_TITBEF,'')) <> ''
GROUP BY 1,2,3 ORDER BY freq DESC LIMIT 20;

-- shoda s referenčním slovníkem: exact vs. po normalizaci
SELECT COUNT(*) radku_s_titulem,
  SUM(EXISTS (SELECT 1 FROM REF_TITBEF r
       WHERE CONVERT(TRIM(r.VALUE) USING utf8mb4) = CONVERT(TRIM(p.PARTY_TITBEF) USING utf8mb4)))
    AS exact_shoda,
  SUM(EXISTS (SELECT 1 FROM REF_TITBEF r
       WHERE LOWER(REPLACE(CONVERT(TRIM(r.VALUE) USING utf8mb4),'.','')) =
             LOWER(REPLACE(CONVERT(TRIM(p.PARTY_TITBEF) USING utf8mb4),'.',''))))
    AS shoda_po_normalizaci
FROM PART_PARTY p WHERE TRIM(COALESCE(p.PARTY_TITBEF,'')) <> '';
```

Reálný poměr: exact 21–29 %, po normalizaci ~81 %. Ten rozdíl je přesně objem práce pro
standardizaci a nejsilnější argument pro nápravné opatření.

---

## 3. Sémantická správnost (SEM_CORR)

```sql
-- mimo číselník (anti-join + collation-safe)
SELECT COUNT(*) mimo_lov FROM <t> a
LEFT JOIN <lov> l ON CONVERT(a.<col> USING utf8mb4) = CONVERT(l.CODE USING utf8mb4)
WHERE a.<col> IS NOT NULL AND l.CODE IS NULL;

-- rozsahy
SELECT SUM(PARTY_AGE < 0) zaporny, SUM(PARTY_AGE > 130) nesmyslny FROM PART_PARTY;

-- sentinel datum
SELECT COUNT(*) sentinel, ROUND(100.0*COUNT(*)/(SELECT COUNT(*) FROM PROD_CONTRACT),2) pct
FROM PROD_CONTRACT WHERE CNTR_VALIDTO >= '2999-01-01';

-- plošná konstanta (pct = 100 → finding)
SELECT <col>, COUNT(*) freq, ROUND(100.0*COUNT(*)/(SELECT COUNT(*) FROM <t>),2) pct
FROM <t> GROUP BY 1 ORDER BY freq DESC LIMIT 5;

-- hodnota patřící do jiného sloupce (IČO v poli sektoru)
SELECT COUNT(*) FROM PART_PARTY WHERE CAST(PARTY_ESA95 AS CHAR) = TRIM(PARTY_CREGNUM);
```

Co regex nechytí: složená hodnota v atomickém poli (`Praha 5 - Smíchov`), celá adresní
hierarchie v poli obce, role místo titulu (`otec`), pevná linka pod typem mobil, zrušený
produkt (`DEL_FLAG=1`) stále v katalogu s platným `VALID_TO`.

---

## 4. Vnitřní konzistentnost (INT_CONS)

```sql
-- odvozený atribut vs. zdroj (uložený věk vs. datum narození)
SELECT SUM(ABS(PARTY_AGE - TIMESTAMPDIFF(YEAR, PARTY_DOFBIRTH, '<snapshot>')) > 1) mismatch,
       MAX(ABS(PARTY_AGE - TIMESTAMPDIFF(YEAR, PARTY_DOFBIRTH, '<snapshot>'))) max_rozdil
FROM PART_PARTY WHERE PARTY_TYPE='P' AND PARTY_AGE IS NOT NULL AND PARTY_DOFBIRTH IS NOT NULL;

-- identifikátor vs. odvozené atributy (RČ ↔ datum narození ↔ pohlaví)
SELECT COUNT(*) FROM PART_PARTY
WHERE PARTY_TYPE='P' AND PARTY_RC REGEXP '^[0-9]{9,10}$'
  AND PARTY_RC NOT IN ('9999999999','0000000000') AND PARTY_DOFBIRTH IS NOT NULL
  AND ( SUBSTRING(PARTY_RC,1,2) <> RIGHT(CAST(YEAR(PARTY_DOFBIRTH) AS CHAR),2)
     OR (PARTY_GENDER='M' AND CAST(SUBSTRING(PARTY_RC,3,2) AS UNSIGNED) <> MONTH(PARTY_DOFBIRTH))
     OR (PARTY_GENDER='F' AND CAST(SUBSTRING(PARTY_RC,3,2) AS UNSIGNED) <> MONTH(PARTY_DOFBIRTH)+50) );

-- vzájemně vylučující se atributy vyplněné zároveň
SELECT SUM(PARTY_TYPE='P' AND TRIM(COALESCE(PARTY_CREGNUM,'')) <> '') fo_s_icem,
       SUM(PARTY_TYPE='C' AND TRIM(COALESCE(PARTY_RC,''))      <> '') po_s_rc,
       SUM(PARTY_TYPE='C' AND TRIM(COALESCE(PARTY_FNAME,'')) NOT IN ('','NA')) po_se_jmenem
FROM PART_PARTY;

-- párové atributy: jeden vyplněn, druhý ne
SELECT SUM(TRIM(COALESCE(ADDR_CITY,''))<>''   AND TRIM(COALESCE(ADDR_ZIP,''))='')  mesto_bez_psc,
       SUM(TRIM(COALESCE(ADDR_ZIP,''))<>''    AND TRIM(COALESCE(ADDR_CITY,''))='') psc_bez_mesta,
       SUM(TRIM(COALESCE(ADDR_STREET,''))<>'' AND TRIM(COALESCE(ADDR_NUM1,''))=''
                                              AND TRIM(COALESCE(ADDR_NUM2,''))='') ulice_bez_cisla
FROM PARTY_ADDRESS;

-- časové rozsahy + forenzní detail
SELECT COUNT(*) obraceny_rozsah,
       MIN(DATEDIFF(CNTR_VALIDFROM,CNTR_VALIDTO)) min_dnu,
       MAX(DATEDIFF(CNTR_VALIDFROM,CNTR_VALIDTO)) max_dnu,
       ROUND(AVG(DATEDIFF(CNTR_VALIDFROM,CNTR_VALIDTO)),1) prumer_dnu
FROM PROD_CONTRACT WHERE CNTR_VALIDFROM > CNTR_VALIDTO;

SELECT YEAR(CNTR_VALIDFROM) rok, COUNT(*) FROM PROD_CONTRACT
WHERE CNTR_VALIDFROM > CNTR_VALIDTO GROUP BY 1 ORDER BY 2 DESC;   -- vše z 1 roku = dávka

-- stav vs. datum: ukončeno, ale platnost do „nekonečna"
SELECT COUNT(*) zrusene_s_nekonecnem FROM PROD_CONTRACT
WHERE CNTR_CANCTYPE IS NOT NULL AND CNTR_VALIDTO >= '2999-01-01';

-- smlouva mimo platnost produktu (křížová kontrola na katalog)
SELECT SUM(c.CNTR_VALIDFROM < p.VALID_FROM) pred_spustenim,
       SUM(c.CNTR_VALIDFROM > p.VALID_TO)   po_zruseni
FROM PROD_CONTRACT c JOIN PRODUCT_CATALOGUE p ON c.PRODUCT_CODE = p.PRODUCT_CODE;
```

---

## 5. Vnější konzistentnost (EXT_CONS)

```sql
-- sirotci
SELECT COUNT(*) sirotku FROM PARTY_CONTACT c
LEFT JOIN PART_PARTY p ON c.PARTY_ID = p.PARTY_ID WHERE p.PARTY_ID IS NULL;

-- forenzní pokračování: tvoří ID sirotků souvislou řadu za posledním ID rodiče?
SELECT MIN(c.PARTY_ID) od, MAX(c.PARTY_ID) do,
       COUNT(DISTINCT c.PARTY_ID) distinct_ids,
       MAX(c.PARTY_ID)-MIN(c.PARTY_ID)+1 range_size,
       (SELECT MAX(PARTY_ID) FROM PART_PARTY) posledni_rodic
FROM PARTY_CONTACT c LEFT JOIN PART_PARTY p ON c.PARTY_ID = p.PARTY_ID
WHERE p.PARTY_ID IS NULL;
-- distinct_ids = range_size a od = posledni_rodic+1 → nedokončený import z jiného systému
```

Tenhle dotaz udělal z „37 879 sirotků" nález s prokázanou příčinou: souvislá řada
383 132–421 010 začínající přesně za posledním klientem. Bez něj je to jen počet.

```sql
-- match rate na referenční registr (metrika, ne jen počet chyb)
SELECT ROUND(100.0*SUM(ADDR_CODE IS NOT NULL)/COUNT(*),2) match_rate_pct FROM PARTY_ADDRESS;

-- pokrytí referenční tabulky (unese roli standardu?)
SELECT COUNT(*) provoz, SUM(r.ICO IS NOT NULL) v_registru,
       ROUND(100.0*SUM(r.ICO IS NOT NULL)/COUNT(*),2) pokryti_pct
FROM PART_PARTY p LEFT JOIN REF_PARTY_CREGNUM r ON TRIM(p.PARTY_CREGNUM) = r.ICO
WHERE p.PARTY_TYPE='C';
```

---

## 6. Unikátnost (UNIQUENESS)

```sql
-- duplicity podle přirozeného klíče
SELECT REPLACE(PARTY_RC,'/','') klic, COUNT(*) cnt FROM PART_PARTY
WHERE PARTY_TYPE='P' AND TRIM(COALESCE(PARTY_RC,'')) <> ''
GROUP BY 1 HAVING cnt > 1 ORDER BY cnt DESC LIMIT 20;

-- kolik skupin celkem (do metriky), ne jen TOP 20
SELECT COUNT(*) dup_skupin, SUM(cnt) dotcenych_radku FROM (
  SELECT PARTY_CREGNUM, COUNT(*) cnt FROM PART_PARTY
  WHERE PARTY_TYPE='C' AND TRIM(COALESCE(PARTY_CREGNUM,'')) <> ''
  GROUP BY 1 HAVING COUNT(*) > 1) z;

-- redundance celého záznamu
SELECT COUNT(*) total,
  COUNT(DISTINCT CONCAT_WS('|',IFNULL(ADDR_STREET,''),IFNULL(ADDR_CITY,''),IFNULL(ADDR_ZIP,''))) unik,
  ROUND(100.0*(COUNT(*) - COUNT(DISTINCT CONCAT_WS('|',IFNULL(ADDR_STREET,''),
        IFNULL(ADDR_CITY,''),IFNULL(ADDR_ZIP,''))))/COUNT(*),2) redundance_pct
FROM PARTY_ADDRESS;

-- dummy hodnoty: jedna hodnota sdílená mnoha entitami
SELECT CONT_VALUE, CONT_TYPE, COUNT(DISTINCT PARTY_ID) klientu FROM PARTY_CONTACT
GROUP BY 1,2 HAVING klientu > 5 ORDER BY klientu DESC LIMIT 20;

-- duplicita kombinace, kterou má hlídat unikátní index (a nehlídá)
SELECT PARTY_ID, CONT_TYPE, COUNT(*) cnt FROM PARTY_CONTACT
GROUP BY 1,2 HAVING cnt > 1 ORDER BY cnt DESC LIMIT 15;
```

Duplicity **neměř před standardizací** — `MuDr` vs `MUDr.`, `Nováková` vs `NOVAKOVA` se jako
duplicita nechytí. Naměřené číslo je dolní odhad; napiš to do zprávy.

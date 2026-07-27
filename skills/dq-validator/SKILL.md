---
name: dq-validator
description: Validace dat a měření metrik kvality v libovolné databázi — kontroly po šesti dimenzích (úplnost, syntaktická a sémantická správnost, vnitřní a vnější konzistentnost, unikátnost) nad provozními tabulkami, číselníky i referenčními registry, checksum validátory identifikátorů (rodné číslo, IČO, VAT, SSN), regex pravidla pro e-mail, telefon a poštovní kód, zápis skóre do metadatového repozitáře. Druhý krok DQ pipeline, po dq-profiler, před dq-auditor. Použij vždy, když se mají "spustit kontroly nad daty", "změřit kvalitu dat", "zvalidovat atributy", "spočítat DQ skóre", "najít nevalidní nebo nekonzistentní záznamy", "ověřit referenční integritu" — i když uživatel neřekne slovo validace. Keywords: validace, data validation, kontroly, DQ metriky, regex validace, checksum, mod 11, referenční integrita, sirotci, DQM_MDR, úplnost, konzistentnost, duplicity.
---

# Validator — kontroly a měření

Profiler řekl, co v datech je. Validator to **spočítá**. Každá kontrola vrací číslo, univerzum
a příslušnost k dimenzi. Bez toho to není měření, ale dojem.

Pipeline: `dq-profiler` → **dq-validator** → `dq-auditor`. Standard v `dq-pipeline`.
Kompletní SQL katalog: `references/checks-sql.md`.

## Pravidla měření

- **Univerzum ke každému číslu.** `SUM(...)` bez `COUNT(*)` a bez filtru na typ entity je
  nepoužitelné. Formát: absolutní počet + procento + z čeho.
- **Anti-join, ne `NOT IN`.** `NOT IN (SELECT ...)` s jediným NULL vrátí prázdnou množinu a
  kontrola tiše projde. Vždy `LEFT JOIN ... WHERE x.KEY IS NULL`.
- **Collation-safe JOIN.** `CONVERT(a.COL USING utf8mb4) = CONVERT(l.CODE USING utf8mb4)`,
  jinak Error 1267 nebo tiché nesparování.
- **Zafixuj snapshot.** Metriky se počítají k datu, ne k `CURDATE()`, jinak nejsou reprodukovatelné.
- **Nula je výsledek.** „0 sirotků" je pozitivní nález a patří do zprávy. Ale připoj, jestli
  je vynucená (FK existuje) nebo náhodná (FK chybí) — bez FK je to stav, ne záruka.
- **Vzorek ke každému nálezu.** Ke každému nenulovému počtu vytáhni 10–20 konkrétních řádků.
  Interpretaci dělá vzorek, ne agregát.

## Postup podle typu tabulky

### Provozní tabulka — všech šest dimenzí

1. **Úplnost** — po sloupcích, s filtrem univerza, rozliš NULL / `''` / zástupnou hodnotu.
2. **Syntaktická správnost** — regex, délka, checksum, whitespace, case; jen na neprázdných.
3. **Sémantická správnost** — anti-join na číselník, rozsahy, sentinel hodnoty, hodnoty
   sémanticky patřící jinam (IČO v poli sektoru, celá adresní hierarchie v poli obce).
4. **Vnitřní konzistentnost** — odvozený vs. zdrojový atribut, vzájemně vylučující se atributy,
   párové atributy, časové rozsahy.
5. **Vnější konzistentnost** — sirotci, napojení na registr (measure match rate).
6. **Unikátnost** — přirozený klíč, redundance celého záznamu, sdílené hodnoty přes N entit.

### Číselník (LOV) — vlastní kvalita + jak se používá

Číselník není součástí univerza, ale **jeho defekty zkreslují měření univerza**, takže se
audituje taky. Šablona (pokryje 90 % nálezů):

```sql
-- kvalita samotného číselníku
SELECT COUNT(*) total,
  SUM(CODE IS NULL OR TRIM(CODE)='')                       code_blank,
  SUM(VALUE IS NULL OR TRIM(VALUE)='')                     value_blank,
  SUM(DESCR IS NULL OR TRIM(DESCR) IN ('','NA'))           descr_prazdny,
  SUM(CODE <> TRIM(CODE))                                  code_whitespace,
  SUM(BINARY TRIM(CODE) <> BINARY UPPER(TRIM(CODE)))       code_not_upper,
  SUM(DEL_FLAG IS NULL OR TRIM(DEL_FLAG) NOT IN ('Y','N')) del_flag_mimo,
  SUM(VALID_FROM > VALID_TO)                               rozsah_obraceny,
  COUNT(DISTINCT TRIM(VALUE))                              distinct_value
FROM <lov>;

-- využití číselníku provozem (obě strany!)
SELECT l.CODE, l.VALUE, COUNT(a.<col>) pouziti
FROM <lov> l LEFT JOIN <provoz> a
  ON CONVERT(a.<col> USING utf8mb4) = CONVERT(l.CODE USING utf8mb4)
GROUP BY 1,2 ORDER BY pouziti;                    -- 0 = mrtvá položka číselníku

SELECT a.<col>, COUNT(*) cnt                       -- hodnoty mimo číselník
FROM <provoz> a LEFT JOIN <lov> l
  ON CONVERT(a.<col> USING utf8mb4) = CONVERT(l.CODE USING utf8mb4)
WHERE a.<col> IS NOT NULL AND l.CODE IS NULL GROUP BY 1 ORDER BY cnt DESC;
```

Co u číselníků kontrolovat vždy:

| Kontrola | Proč | Reálný nález |
|---|---|---|
| kódy použité v provozu, ale chybějící v číselníku | root cause = chybí FK | typ adresy `P` u 382 210 adres (40 %) mimo číselník; kódy frekvence `3` a `5` u 91 693 smluv |
| kódy v číselníku bez použití | mrtvá položka nebo důsledek jiného defektu | 239 z 240 kódů zemí nepoužito (kvůli plošnému `CZE`); kategorie produktu 7 bez produktu i smlouvy |
| duplicitní `VALUE` pod různými `CODE` | rozdvojení statistik | `Dipl.tech.` pod CODE 19 i 20; `M.S.` a `MSc` se stejným popisem |
| prázdný popisný sloupec | datový slovník fakticky neexistuje | `DESC`/`DESCR` NULL nebo `'NA'` u 7 z 9 číselníků |
| nekonzistentní název popisného sloupce | generické ETL musí ošetřit obě varianty | 4× `DESC` vs 5× `DESCR` pro identický atribut |
| nekonzistentní `DEL_FLAG` | tři implementace téhož příznaku | `char(1)` Y/N vs `tinyint` 0/1 vs `char(4)` |
| nekonzistentní sentinel `VALID_TO` | různé „nekonečno" ztěžuje filtr platnosti | `2999-01-01` vs `2999-12-31` vs NULL |
| zastaralé položky bez deaktivace | číselník neodráží realitu | zaniklé státy `ANT`, `SCG` aktivní; přejmenované `SWZ`, `MKD`, `LBY` se starým názvem |
| překlepy v hodnotách | propagují se do reportů a UI | `Corespondence`, `HERZEGOWINA`, `Bacherol` |
| oříznuté hodnoty | `varchar(n)` nestačí na plný název | 9 ze 43 kódů ESA95 s oříznutým `VALUE` |
| zastaralost celé klasifikace | v rozporu s regulací | ESA95 (od 2014 ESA2010), OKEČ (od 2008 CZ-NACE), UIR-ADR (od 2012 RÚIAN) |

### Referenční tabulka (REF) — použitelnost jako měřítko

U referenčního registru se neptej „jsou v něm chyby", ale **„unese vůbec roli standardu?"**:

```sql
-- pokrytí: kolik provozních hodnot registr vůbec zná
SELECT COUNT(*) provoz_total,
       SUM(r.KEY IS NOT NULL) v_registru,
       ROUND(100.0*SUM(r.KEY IS NOT NULL)/COUNT(*),2) pokryti_pct
FROM <provoz> p LEFT JOIN <ref> r ON <join podmínka>;
```

Reálné nálezy: registr IČO měl 1 000 řádků pro 119 348 firemních klientů (pokrytí < 1 %,
98,31 % klientů mimo registr) — nepoužitelné torzo, validace neproveditelná. Adresní registr
měl 2,6 M řádků, ale 88,7 % bez souřadnic a 292 878 s invertovaným znaménkem; sesterská
tabulka postrádala 49 670 platných adres (1,91 %), měla 21 145 řádků s číslicí v poli ulice
(porušení atomičnosti v samotném referenčním zdroji) a **z principu neobsahovala P.O. Boxy**.

Poslední bod je důležitý žánr nálezu: **rozsahová mezera zdroje**. Klientská adresa s poštovní
přihrádkou je formálně platná, ale vůči tomuto registru bude vždy nevalidní. To není defekt
klientských dat — a musí to být ve zprávě napsané, jinak se 100 % těchto řádků vykáže jako chyba.

## Doménové validátory (příklad: ČR)

Následující pravidla jsou vázaná na českou jurisdikci. **Struktura kontroly je univerzální**
(formát → checksum → křížová konzistence s odvozenými atributy), konkrétní váhy a délky nahraď
ekvivalentem cílové země — mapovací tabulka je v `dq-pipeline`, sekce Přenositelnost.

| Atribut | Pravidlo |
|---|---|
| **Rodné číslo** | 9 nebo 10 číslic po odstranění `/`. 10místné validní když `RC % 11 = 0`, nebo (historická výjimka) `prvních9 % 11 = 10` a poslední číslice `0`. 9místné (do 1953) kontrolní číslici nemá — validuj jen strukturu. Měsíc: `+50` žena, `+20`/`+70` přidělovací navýšení (od 2004). |
| **IČO** | 8 číslic (doplň vedoucí nuly). Mod 11: váhy 8..2 na prvních 7 číslic, `zbytek = součet % 11`, kontrolní číslice `(11 - zbytek) % 10`. |
| **PSČ (CZ)** | `^[0-9]{5}$` bez mezer. Pozor na `char(5)` — vstup s mezerou se useknul. |
| **E-mail** | `^[A-Za-z0-9._+%-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,24}$`. Kontroluj zvlášť `#`, `&` a mezeru uvnitř — to nejsou překlepy, ale systematická záměna. |
| **Mobil (CZ)** | 9 číslic po strip `+420`/`00420`, první číslice 6 nebo 7. |
| **Pevná (CZ)** | 9 číslic, první číslice 2–5. Číslo s předvolbou 2xx/5xx vedené jako mobil = sémantická chyba, ne syntaktická. |
| **Kód země** | ISO 3166-1 alpha-3, tři velká písmena. |

Placeholdery, které musí spadnout do „nevalidní" ještě před regexem:
`''`, `NA`, `N/A`, `NULL`, `.`, `-`, `cizinec`, `NEVYPLNENO`, `9999999999`, `0000000000`,
`99999`, `00000`, číslo tvořené jednou opakovanou číslicí.

## Zápis do DQM_MDR

Metadatový repozitář = `information_schema` rozšířený o sloupce pro naměřené dimenze
(`COMPLETENESS`, `SYN_CORR`, `SEM_CORR`, `INT_CONS`, `EXT_CONS`). Jeden řádek = jeden sloupec
jedné tabulky. Skóre je podíl v intervalu 0–1, počítaný **vůči relevantnímu univerzu**:

```sql
UPDATE DQM_MDR SET COMPLETENESS = (
  SELECT 1 - ( SUM(CASE WHEN PARTY_TYPE='P' AND (PARTY_FNAME IS NULL OR TRIM(PARTY_FNAME)='')
                        THEN 1 ELSE 0 END)
               / NULLIF(SUM(CASE WHEN PARTY_TYPE='P' THEN 1 ELSE 0 END), 0) )
  FROM PART_PARTY )
WHERE TABLE_NAME = 'PART_PARTY' AND COLUMN_NAME = 'PARTY_FNAME';
```

Pokrytí MDR je samo o sobě metrika. Když repozitář eviduje 39 sloupců ze 4 tabulek, zatímco
databáze má 23 tabulek, je to finding („MDR nepokrývá univerzum") a zároveň úkol — rozšířit ho.

Ne každá dimenze dává smysl u každého sloupce. Nevyplňuj nulou, nech NULL a uveď proč.

## Výstup validatoru

Na tabulku jedna sekce `## 3. Audit` členěná přesně po dimenzích (Úplnost, Syntaktická
správnost, Sémantická správnost, Unikátnost, Vnitřní konzistentnost, Vnější konzistentnost,
Zápis DQ skóre). Ke každé: kritérium, dotaz, naměřené číslo, vzorek, jednovětná interpretace.

Interpretace musí být konkrétní. „Spouští SQL" nebo „zpracovává data" není interpretace —
napiš, co se validuje, kolik toho neprošlo a co to pro byznys znamená.

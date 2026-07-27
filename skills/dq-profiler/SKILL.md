---
name: dq-profiler
description: Technický profiling databáze — inventura tabulek, struktura a datové typy, collation, PK/FK/indexy, frekvenční distribuce, vzorky, detekce vzorců defektů (plošné konstanty, sentinel hodnoty, padding, useknuté hodnoty, sirotčí řady ID). První krok DQ pipeline, před dq-validator. Použij, když se má "podívat na databázi", "zjistit co v těch datech je", "profiling", "průzkum dat", "co je s tou databází špatně". Keywords: profiling, data profiling, inventura, struktura tabulek, collation, distribuce hodnot, průzkum databáze.
---

# Profiler — technický profiling

První krok. **Nehledej ještě chyby, hledej tvar dat.** Chyby najde `dq-validator` až poté, co
víš, co v tabulkách vůbec je. Profiler končí seznamem tabulek, jejich rolí a seznamem
podezření, které validator ověří.

Pipeline: **dq-profiler** → `dq-validator` → `dq-auditor` → remediace. Společný standard
(dimenze, formát zjištění) je v `dq-pipeline`.

## Krok 1 — inventura a role tabulek

Roztřiď všechny tabulky do tří košů. Bez toho nevíš, co je univerzum auditu a co referenční
standard, kterým se univerzum měří:

| Koš | Poznávací znak | Role v auditu |
|---|---|---|
| **provozní** | transakční/kmenová data, roste v čase | univerzum auditu |
| **číselník (LOV)** | pár desítek řádků, `CODE`+`VALUE`, interní doména | měřítko pro SEM_CORR |
| **referenční (REF)** | externí standard nebo registr, statisíce až miliony řádků | měřítko pro EXT_CONS |

Prefix v názvu neber jako pravdu, ověř obsahem. V auditované DB byly `REF_TITBEF`, `REF_FNAME`,
`REF_NACE` strukturou obyčejné číselníky (LOV), zatímco `LOV_COUNTRY` a `LOV_ESA95` byly
externí standardy (ISO 3166-1, ESA95) → měly být REF. Záměna prefixů je sama o sobě finding.

```sql
SELECT TABLE_NAME, TABLE_ROWS, ENGINE, TABLE_COLLATION
FROM information_schema.TABLES WHERE TABLE_SCHEMA = DATABASE() ORDER BY TABLE_ROWS DESC;
```

`TABLE_ROWS` je u InnoDB odhad. Pro metriky vždy `COUNT(*)`.

## Krok 2 — struktura, typy, collation

```sql
SELECT TABLE_NAME, COLUMN_NAME, DATA_TYPE, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT,
       CHARACTER_SET_NAME, COLLATION_NAME
FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE()
ORDER BY TABLE_NAME, ORDINAL_POSITION;
```

Co číst z výstupu:

| Signál | Co znamená | Reálný příklad |
|---|---|---|
| `char(n)` na proměnlivě dlouhém textu | padding mezerami + tiché useknutí delšího vstupu | `PARTY_TITBEF char(10)` → `'PhDr      '`; `ADDR_ZIP char(5)` + vstup `251 62` → uloženo `251 6` |
| `char(4)` / `tinyint` / `char(1)` pro tentýž Y/N příznak | tři implementace téhož konceptu | `DEL_FLAG` napříč 9 číselníky |
| nullable u logicky povinného sloupce | chybí constraint, aplikace to nehlídá | `PARTY_TYPE` nullable |
| `NOT NULL` + prázdný default | „vyplněno" znamená prázdný řetězec | `ADDR_TYPE NOT NULL DEFAULT ''` |
| více collation v jedné tabulce | JOIN spadne na Error 1267 nebo tiše nesparuje | `ADDR_CITY utf8mb3_czech_ci` vs `ADDR_TYPE utf8mb4_0900_ai_ci` |
| číselný typ na kódu s vedoucí nulou | ztráta nul (IČO, PSČ) | `ADDR_ZIP_STD integer` |
| lokalizované zkratky v jinak anglickém schématu | naming convention porušena selektivně | `PARTY_RC`, `PARTY_OKEC` |

Collation zmapuj **jako matici** — pro každou plánovanou JOIN vazbu porovnej collation obou
stran. Nesoulad zapiš jako finding hned, netahej ho do validatoru jako překvapení.

```sql
SELECT COLLATION_NAME, COUNT(*) sloupcu, GROUP_CONCAT(DISTINCT TABLE_NAME) tabulky
FROM information_schema.COLUMNS
WHERE TABLE_SCHEMA = DATABASE() AND COLLATION_NAME IS NOT NULL GROUP BY 1;
```

## Krok 3 — klíče, indexy, referenční integrita

```sql
-- PK a unikátní indexy
SELECT TABLE_NAME, INDEX_NAME, NON_UNIQUE,
       GROUP_CONCAT(COLUMN_NAME ORDER BY SEQ_IN_INDEX) cols
FROM information_schema.STATISTICS WHERE TABLE_SCHEMA = DATABASE()
GROUP BY 1,2,3 ORDER BY 1;

-- deklarované FK
SELECT TABLE_NAME, COLUMN_NAME, REFERENCED_TABLE_NAME, REFERENCED_COLUMN_NAME
FROM information_schema.KEY_COLUMN_USAGE
WHERE TABLE_SCHEMA = DATABASE() AND REFERENCED_TABLE_NAME IS NOT NULL;
```

**Prázdný výsledek druhého dotazu je kritický nález, ne technikálie.** V auditované DB nemělo FK
ani jedno z 9 číselníků a 8 referenčních tabulek — jediným zdrojem integrity byla aplikační
vrstva, která prokazatelně selhávala (91 693 smluv s kódem frekvence mimo číselník,
382 210 adres s typem mimo číselník, 37 879 osiřelých kontaktů).

Sestav **seznam chybějících FK** — každá vazba, kterou datový model implikuje, ale schéma
nevynucuje. Ten seznam je příloha zprávy a zároveň zadání pro `dq-strazce`.

Pozor i na PK: surrogate `auto_increment` PK ještě neznamená unikátnost byznysovou.
`PARTY_ADDRESS` měla PK na `ADDR_ID`, ale nic nebránilo uložit tutéž adresu klientovi 3×.
Jednoznakový PK (`CODE char(1)`) je křehký — při rozšíření číselníku dojde ke kolizi.

## Krok 4 — objem a základní charakteristiky

Pro každou provozní tabulku: `COUNT(*)`, rozpad podle typu entity, min/max u datumů a čísel,
počet distinct u kandidátů na kategorii.

```sql
SELECT PARTY_TYPE, COUNT(*) freq FROM PART_PARTY GROUP BY 1 ORDER BY freq DESC;
SELECT MIN(CNTR_VALIDFROM), MAX(CNTR_VALIDFROM), MIN(CNTR_VALIDTO), MAX(CNTR_VALIDTO)
FROM PROD_CONTRACT;
```

`MAX` u datumu je jeden z nejlevnějších detektorů sentinelů — když vyjde `2999-12-31`
nebo `3000-01-01`, máš rovnou nález i vysvětlení.

## Krok 5 — frekvenční distribuce (jádro profilingu)

Pro každý kategoriální a podezřelý sloupec:

```sql
SELECT <col>, COUNT(*) freq, ROUND(100.0*COUNT(*)/(SELECT COUNT(*) FROM <t>),2) pct
FROM <t> GROUP BY 1 ORDER BY freq DESC LIMIT 20;

-- délková distribuce odhalí padding i useknutí
SELECT LENGTH(<col>) len, COUNT(*) freq FROM <t> GROUP BY 1 ORDER BY 1;

-- distribuce dne v měsíci odhalí day/month swap při importu
SELECT DAY(<date_col>) den, COUNT(*) FROM <t> GROUP BY 1 ORDER BY 1;

-- distribuce roku odhalí dávkový import
SELECT YEAR(<date_col>) rok, COUNT(*) FROM <t> GROUP BY 1 ORDER BY 2 DESC LIMIT 15;
```

Nepřirozený peak v jednom roce = migrace nebo dávkový import. V auditované DB mělo 26 000+
klientů rok narození 2010 a **všech** 31 764 antedatovaných smluv pocházelo z roku 2012.

## Krok 6 — vzorek dat očima, ne dotazem

`SELECT * FROM <t> LIMIT 5` a pořádně se podívej. Prvních pět řádků `PARTY_ADDRESS` ukázalo
naráz: prázdné `ADDR_NUM1/NUM2`, číslo domu slité do `ADDR_STREET` („Na Budíně 854"),
konstantní `CZE` a duplicitní adresu u dvou různých klientů. Čtyři nálezy z jednoho pohledu.

## Katalog vzorců defektů

Tohle hledej v distribucích a vzorcích. Každý řádek je hypotéza pro `dq-validator`.

| Vzorec | Jak vypadá | Kořenová příčina | Ověření |
|---|---|---|---|
| **Plošná konstanta** | jedna hodnota u 100 % řádků | sloupec plněn defaultem, ne ze vstupu | křížová kontrola s atributem, který konstantě odporuje |
| **Zástupná hodnota místo NULL** | `NA`, `NEVYPLNENO`, `cizinec`, `9999999999`, `.` | vstupní pole je NOT NULL, operátor musí něco napsat | frekvenční distribuce, TOP hodnoty |
| **Sentinel datum** | `2999-12-31`, `2999-01-01`, `1900-01-01`, `3000-01-01` | systém neumí NULL / „nekonečná platnost" | `MAX()`/`MIN()` na datumech |
| **Padding na fixní délku** | `'PhDr      '`, délka přesně `n` | import do `char(n)` bez trimu | délková distribuce |
| **Useknutá hodnota** | `251 6`, `400 0` místo `251 62` | `char(5)` + vstup s mezerou | regex validace + vzorek |
| **Prokládané mezery** | `D A V I D` | ruční přepis nebo rozbitý parser | `REGEXP '^([A-Za-z] )+'` |
| **Kódovací chyba** | `ĄUDOVÍT` (má být `ĽUDOVÍT`), `BE0ŠOVÁ` | špatná znaková sada při migraci | hledej číslice ve jménech a nečeské znaky |
| **Systematická záměna znaku** | `gmail#com`, `gmail&cz` | mapování znaků při importu | počítej výskyt podezřelého znaku, ne jen „nevalidní" |
| **Sirotci v souvislé řadě ID** | `range_size = distinct_ids`, start = `max(parent)+1` | nedokončený dávkový import z jiného systému | `MIN`/`MAX`/`COUNT(DISTINCT)` na osiřelých ID |
| **Stínová taxonomie** | kódy mimo číselník, ale masově použité | obchodníci si zavedli vlastní hodnoty | anti-join na číselník + četnost |
| **Single-table pattern** | dva typy entit v jedné tabulce bez podtypování | chybí subtyping | rozpad completeness podle typu |
| **Antedatování** | `valid_from > valid_to`, všechno z jednoho roku | dávkový import / pozdní integrace kanálu | distribuce roku u chybných řádků |
| **Day/month swap** | dny 13–31 podezřele řídké | locale MM/DD vs DD/MM při importu | distribuce dne v měsíci |
| **Redundantní sloupec** | `VALUE` == `DESCR` u 100 % řádků | kopie při návrhu, jeden se přestal udržovat | `SUM(VALUE = DESCR)` |
| **Duplicitní kód pro tutéž hodnotu** | `Dipl.tech.` pod CODE 19 i 20 | ruční správa číselníku | `GROUP BY TRIM(VALUE) HAVING COUNT(*)>1` |
| **Mrtvá položka číselníku** | kód existuje, 0 použití | zrušený produkt / rezerva / překlep | LEFT JOIN z číselníku na provoz |
| **Torzo referenční tabulky** | 1 000 řádků tam, kde má být registr | vadný import nebo omezený vzorek od dodavatele | pokrytí = kolik provozních hodnot je v REF |

## Výstup profileru

Jeden dokument (nebo notebook) na tabulku, jednotná struktura:

```
# <TABULKA>
Název (co zkratka znamená) · O tabulce · Role v datovém modelu · Návaznosti
## 2. Struktura a obsah
  2.1 Přehled sloupců (typ, nullable, collation) + co z toho plyne
  2.2 Základní charakteristiky (počty, rozpad podle typu entity)
  2.3 Ukázka dat + co je na ní vidět
## Podezření pro validaci
  seznam hypotéz s odkazem na vzorec z katalogu
```

Sekci „Role v datovém modelu" nevynechávej — bez ní se v dalších krocích nepozná, které
číselníky a registry jsou pro tabulku měřítkem.

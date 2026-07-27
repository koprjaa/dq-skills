---
name: dq-audit
description: Vstupní bod a společný standard pipeline pro řízení kvality dat v libovolné databázi (MySQL, PostgreSQL, SQL Server, Oracle, SQLite, DuckDB) — DQ dimenze, formát zjištění, škála závažnosti, mapování schématu na role, přenositelnost SQL a doménových pravidel. Použij, když se má "udělat audit databáze", "zkontrolovat kvalitu dat", "vyčistit databázi", "najít chyby v datech", nebo když nevíš, kterým krokem pipeline začít. Keywords: audit kvality dat, data quality, DQ, profiling, zpráva auditora, nápravná opatření, čištění dat, data cleansing.
---

# DQ pipeline — mapa a společný standard

Deset skillů, dvě fáze. **Audit** zjistí a vyčíslí stav, **remediace** ho opraví.
Pipeline je nezávislá na doméně i na databázovém stroji — viz Přenositelnost níže.

```
dq-profiler → dq-validator → dq-auditor                    (audit: co je špatně, kolik to stojí)
     ↓
dq-parser → dq-standardizator → dq-adresar → dq-imputator → dq-deduplikator → dq-strazce
                                                            (remediace: oprava + prevence)
```

| Skill | Co dělá | Kdy |
|---|---|---|
| `dq-profiler` | inventura, struktura, collation, PK/FK, distribuce, vzorce defektů | vždy první |
| `dq-validator` | kontroly po dimenzích, měření metrik, zápis do metadatového repozitáře | po profileru |
| `dq-auditor` | katalog zjištění, root-cause, COPQ/ROI, prioritizace, zpráva | po validatoru |
| `dq-parser` | atomizace složených hodnot | první krok remediace |
| `dq-standardizator` | kanonický tvar + napojení na referenční slovníky | po parseru |
| `dq-adresar` | napojení na adresní/geografický registr, match rate | po standardizaci |
| `dq-imputator` | doplnění chybějících hodnot | po napojení na registry |
| `dq-deduplikator` | match code, klastry, survivor, golden record, household | po imputaci |
| `dq-strazce` | typy, FK/CHECK, DQ firewall, monitoring, governance | poslední |

Pořadí není libovolné. Deduplikace před standardizací nesloučí `MuDr` a `MUDr.`. Imputace před
parsingem doplní hodnotu do pole, které se pak stejně rozpadne. Constrainty před remediací
neprojdou, protože je stávající data poruší.

Kroky lze vynechat, když je doména nemá (žádné adresy → bez `dq-adresar`), ale **nikdy nepřehazuj
pořadí**.

## DQ dimenze — každé zjištění patří právě do jedné

| Dimenze | Sloupec v MDR | Otázka |
|---|---|---|
| Úplnost | `COMPLETENESS` | Je hodnota vyplněná vůči **relevantnímu univerzu**? |
| Syntaktická správnost | `SYN_CORR` | Odpovídá formát pravidlu (regex, délka, checksum)? |
| Sémantická správnost | `SEM_CORR` | Dává hodnota v kontextu smysl (číselník, rozsah, realita)? |
| Vnitřní konzistentnost | `INT_CONS` | Sedí atributy téhož záznamu spolu? |
| Vnější konzistentnost | `EXT_CONS` | Sedí s jiným zdrojem (FK, číselník, externí registr)? |
| Unikátnost | `UNIQUENESS` | Kolik řádků je duplicitních / kolik entit reálně existuje? |

Existuje i **strukturální** varianta („syntaktická správnost struktury"): defekt není v hodnotě,
ale ve schématu — `char(4)` pro Y/N příznak, jednoznakový PK, chybějící FK, smíšená collation.
Klasifikuj ji do dimenze, jejíž měření kazí, a v popisu uveď „(struktura)".

## Univerzum je povinné

„5 328 chybí" nic neznamená. „5 328 z 263 783 fyzických osob (2,02 %); 119 348 právnických
osob je legitimně NULL" znamená. Před každým počítáním úplnosti si odpověz, pro koho je
atribut vůbec relevantní.

Klasická past: jedna tabulka drží dva typy entit bez podtypování. Sloupce specifické pro jeden
typ jsou u druhého buď NULL, nebo vyplněné zástupným `NA` — a obojí zkresluje metriku opačným
směrem. Vždy filtruj na typ entity.

## Formát zjištění

| ID | Atribut | Dimenze | Zjištění (s čísly) | Závažnost | Byznys dopad |
|---|---|---|---|---|---|
| A1 | PARTY_COUNTRY | SEM_CORR | Konstanta „CZE" u 100 % z 383 131 záznamů | Kritické | AML screening nefunkční, regulatorní reporting zkreslen |

Kódování: písmeno = doména, číslo = pořadí, přípona `b` = křížová kontrola k témuž jevu.

| Stupeň | Kritérium |
|---|---|
| **Kritické** | porušení regulace, neidentifikovatelná osoba, přímá finanční ztráta, nefunkční klíčový proces |
| **Vysoké** | proces funguje jen částečně, měřitelná ztráta příležitosti, bez opravy se defekt množí |
| **Střední** | zhoršuje analytiku a údržbu, obchází se ručně |
| **Nízké** | hygiena, kosmetika, konzistence pojmenování |

## Křížová kontrola je nejcennější nález

Jedna anomálie je chyba. Dvě propojené jsou **důkaz o příčině**.

Příklad: sloupec země je konstantně „CZE" — podezřelé. Druhý dotaz: 2 633 klientů má v poli
identifikátoru literál „cizinec" a **u všech 2 633 (100 %)** je země „CZE". Tím je dokázáno,
že sloupec není odvozován ze vstupu, ale plošně defaultován. Z podezření se stala evidence.

Ke každému velkému nálezu hledej druhý dotaz, který příčinu potvrdí nebo vyloučí.

## Přenositelnost — mapování libovolného schématu

Skilly popisují **role**, ne konkrétní tabulky. Než začneš, namapuj schéma:

| Role v pipeline | Co to je | Příklady v jiných doménách |
|---|---|---|
| **kmenová entita** | subjekt, o kterém vše ostatní vypovídá | klient, pacient, student, dodavatel, zařízení, občan |
| **atributové satelity** | 1:N detaily entity | adresy, kontakty, bankovní spojení, přístupy |
| **transakce** | události v čase s platností | smlouvy, objednávky, návštěvy, zápisy, měření |
| **katalog** | co se prodává/nabízí, s časovou platností | produkty, služby, předměty, typy zákroků |
| **číselník (LOV)** | interní doména povolených hodnot | typy, stavy, kategorie, příznaky |
| **referenční registr (REF)** | externí autoritativní zdroj | registr adres, osob, klasifikace odvětví, ISO číselníky |

Příklady v ostatních skillech pocházejí z auditu pojišťovny (`PART_PARTY`, `PARTY_ADDRESS`,
`PARTY_CONTACT`, `PROD_CONTRACT`, `LOV_*`, `REF_*`) — ber je jako ilustraci role, ne jako
požadavek na názvy. Vzorce defektů jsou doménově nezávislé; sentinel datum a plošná konstanta
vypadají stejně v pojišťovně, e-shopu i nemocnici.

### SQL dialekty

SQL v katalozích je psané pro MySQL. Překlad:

| Konstrukce | MySQL | PostgreSQL | SQL Server | Oracle | SQLite / DuckDB |
|---|---|---|---|---|---|
| regex | `x REGEXP 'p'` | `x ~ 'p'` | `x LIKE` / CLR | `REGEXP_LIKE(x,'p')` | `regexp_matches` (DuckDB) |
| null coalesce | `IFNULL` | `COALESCE` | `ISNULL` | `NVL` | `COALESCE` |
| rozdíl let | `TIMESTAMPDIFF(YEAR,a,b)` | `EXTRACT(YEAR FROM age(b,a))` | `DATEDIFF(year,a,b)` | `MONTHS_BETWEEN/12` | `julianday` |
| podmíněný součet | `SUM(podmínka)` | `COUNT(*) FILTER (WHERE …)` | `SUM(CASE WHEN … THEN 1 ELSE 0 END)` | totéž | totéž |
| collation fix | `CONVERT(x USING utf8mb4)` | není potřeba | `COLLATE` | `NLS_COMP` | není potřeba |
| metadata | `information_schema` | `information_schema` / `pg_catalog` | `sys.*` | `ALL_TAB_COLUMNS` | `pragma_table_info` |
| hash | `MD5(x)` | `md5(x)` | `HASHBYTES('MD5',x)` | `STANDARD_HASH` | `md5(x)` |
| konkatenace | `CONCAT_WS` | `\|\|` / `concat_ws` | `+` / `CONCAT_WS` | `\|\|` | `\|\|` |

Kde SQL nestačí (fuzzy match, rekonstrukce hodnot, klastrování), přejdi do Pythonu — pandas
příklady jsou v `dq-standardizator` a `dq-deduplikator`. Hranice: agregace a kontroly patří do
SQL (běží u dat), transformace vyžadující slovníky a regex-heavy logiku do Pythonu.

### Doménová pravidla

Validátory identifikátorů jsou vázané na jurisdikci. Nahraď je ekvivalenty cílové domény:

| Vzor | ČR | Jinde |
|---|---|---|
| identifikátor osoby s checksumem | rodné číslo (mod 11) | SSN, NIN, personnummer, fiscal code |
| identifikátor firmy s checksumem | IČO (mod 11) | VAT ID, EIN, company number |
| poštovní kód | 5 číslic | ZIP+4, postcode s písmeny, alfanumerické |
| adresní registr | RÚIAN (ČÚZK) | národní address registry, USPS, ordnance survey |
| registr osob/firem | ROS / ARES | company house, business register |
| klasifikace odvětví | CZ-NACE | NACE, SIC, NAICS |
| kód země | ISO 3166-1 alpha-3 | totéž (mezinárodní) |
| telefon | 9 číslic, prefix 6/7 mobil | E.164 + národní plán |

Konstrukce checksumu (váhy, modulo) se liší; **struktura kontroly zůstává**: formát → checksum
→ křížová konzistence s odvozenými atributy. Deduplikační de-gendering příjmení je jazykově
specifický — pro neflektivní jazyky ho vypusť.

## Tvrdá pravidla

- Nikdy nekonstatuj kvalitu bez dotazu. Nejde-li ověřit, napiš „neověřeno" a proč.
- Žádné tiché coercion, které schová defekt (`CAST` bez kontroly, `COALESCE` na 0).
- 100% konstanta ve sloupci = vždy finding, ne „čistá data".
- Metriky stabilní při rerunu — zafixuj snapshot datum, nepoužívej `CURRENT_DATE` v metrikách.
- Anti-join (`LEFT JOIN … WHERE … IS NULL`), ne `NOT IN` — `NOT IN` s NULL vrátí prázdno.
- Nikdy nepřepisuj původní sloupec. Remediace jde do `*_STD` sloupců, originál zůstává jako
  evidence pro before/after.
- Pozitivní nález je taky nález: „0 sirotků, referenční integrita zachována" patří do zprávy —
  ale rozliš, jestli je vynucená (FK existuje), nebo náhodná (FK chybí).
- Read-only přístup ber jako výchozí. Zápis do produkce jen po explicitním schválení, vždy
  do nových sloupců a se zálohou originálu.

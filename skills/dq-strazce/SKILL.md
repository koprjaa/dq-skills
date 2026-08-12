---
name: dq-strazce
description: Prevence vzniku chyb a monitoring kvality dat — cizí klíče a CHECK constrainty, opravy datových typů, DQ firewall na vstupu, kontinuální měření metrik s alertingem, metadatový repozitář, data governance (vlastnictví dat, stewardi, datový katalog). Poslední krok DQ pipeline, po dq-deduplikator. Použij, když se má "zabránit opakování chyb", "zavést kontroly na vstupu", "nastavit monitoring kvality dat", "přidat FK a constrainty", "data governance". Keywords: prevence, DQ firewall, constraint, foreign key, monitoring, alerting, data governance, data steward, datový katalog, observability.
---

# Strážce — prevence a monitoring

Vyčištěná data bez opravené příčiny se znovu zaneřádí. Tenhle krok je jediný, který má
trvalý efekt — všechno předchozí je jednorázová úklidová akce.

Pipeline: `dq-deduplikator` → **dq-strazce**. Vstup: seznam chybějících FK z `dq-profiler`
a root-cause z `dq-auditor`.

## Čtyři vrstvy obrany

| Vrstva | Kde | Chytí | Cena |
|---|---|---|---|
| 1. datový typ | DDL | strukturální nesmysly (padding, useknutí, ztráta nul) | jednorázová migrace |
| 2. constraint | DB | neplatné hodnoty, sirotky, duplicity | nízká, ale odmítne existující vadná data |
| 3. DQ firewall | aplikace / ETL | vše ostatní, včetně křížových pravidel | střední, nutná údržba |
| 4. monitoring | mimo transakci | co proteklo, a trendy | nízká, ale reaguje až zpětně |

Nasazuj odspodu. Constraint bez opravených dat neprojde — proto je tenhle krok **až po
remediaci**, ne před ní.

## 1. Datové typy

| Vada | Oprava | Proč |
|---|---|---|
| `char(n)` na proměnlivém textu | `varchar(n)` | zdroj paddingu a tichého useknutí — ale u kódu s garantovanou pevnou délkou (IČO `char(8)`) je `char` správně |
| pevná délka na kódu, který se může rozšířit | delší `varchar` | poštovní kód `char(5)` architektonicky vylučuje zahraniční adresy |
| číselný typ na kódu s vedoucí nulou | `varchar` | ztráta nul u identifikátorů a poštovních kódů |
| textový příznak Y/N | `BOOLEAN NOT NULL DEFAULT` | tři různé implementace téhož (`char(1)` / `char(4)` / `tinyint`) |
| jednoznakový PK na číselníku | surrogate `auto_increment` + `UNIQUE` na kódu | křehké, kolize při rozšíření |
| nullable u logicky povinného sloupce | `NOT NULL` | „povinné" musí vynucovat schéma, ne dokumentace |
| smíšená collation | jednotná napříč celou DB | tiché nesparování a chyby při JOIN |

Sjednocení collation dělej **globálně, ne po tabulkách** — jinak vznikne nová dvojice, která
spolu neumí JOIN.

## 2. Constrainty

```sql
-- referenční integrita (nejdřív ověř, že data projdou)
SELECT COUNT(*) FROM child c LEFT JOIN parent p ON c.PARENT_ID = p.ID WHERE p.ID IS NULL;
ALTER TABLE child ADD CONSTRAINT fk_child_parent
  FOREIGN KEY (PARENT_ID) REFERENCES parent(ID);

-- doména hodnot přes číselník
ALTER TABLE PARTY_ADDRESS ADD CONSTRAINT fk_addr_type
  FOREIGN KEY (ADDR_TYPE) REFERENCES LOV_ADDR_TYPE(CODE);

-- byznysová unikátnost (surrogate PK ji nezaručuje)
ALTER TABLE PARTY_CONTACT ADD CONSTRAINT uq_party_cont UNIQUE (PARTY_ID, CONT_TYPE);

-- logická pravidla
ALTER TABLE PROD_CONTRACT ADD CONSTRAINT ck_valid_range
  CHECK (CNTR_VALIDTO IS NULL OR CNTR_VALIDFROM <= CNTR_VALIDTO);
ALTER TABLE PART_PARTY ADD CONSTRAINT ck_rc_format
  CHECK (PARTY_RC IS NULL OR PARTY_RC REGEXP '^[0-9]{9,10}$');
```

FK přidávej **jako celý seznam z profileru**, ne namátkou. Databáze, kde nemá FK ani jedna
z 9 číselníkových a 8 referenčních vazeb, se neopraví třemi constrainty.

Než FK přidáš, rozhodni, co s existujícími porušeními — smazat, přiřadit, nebo rozšířit
číselník. To rozhodnutí patří do zprávy, protože je nevratné.

Constraint, který nejde zavést, je taky výsledek: „na `CNTR_CANCTYPE` nelze zavést FK, protože
pro atribut neexistuje číselník" → napřed návrh číselníku, pak constraint.

## 3. DQ firewall

Validace na vstupu, dřív než se hodnota uloží. Co má být za pravidla, už víš z auditu —
každý nález je jedno pravidlo.

| Kategorie | Pravidlo | Nález, který ho vyvolal |
|---|---|---|
| formát | checksum identifikátoru (mod 11), regex e-mailu, PSČ, telefonu | text `cizinec` v poli rodného čísla |
| doména | hodnota musí existovat v číselníku | 91 693 transakcí s kódem mimo číselník |
| křížová | odvozený atribut musí sedět se zdrojem | 24 642 osob s nesouladem identifikátor ↔ datum narození |
| časová | datum transakce v intervalu platnosti produktu | 23 477 smluv mimo platnost produktu |
| rozsah | věk 0–130, datum narození ne v budoucnosti | uložený věk s odchylkou až 83 let |
| povinnost | podle typu entity, ne globálně | jinak firma musí vyplnit datum narození |
| zákaz zástupných hodnot | blacklist `NA`, `9999999999`, `99999` | zástupné hodnoty jako substitut NULL |
| deduplikace na vstupu | porovnat nový záznam s existující bází (MDM hub) | duplicity zakládané paralelně z více kanálů |

Poslední řádek je nejdražší a nejúčinnější. Bez centrálního porovnání při zakládání entity
se duplicity vyrábějí rychleji, než je stíháš čistit.

Firewall musí umět **odmítnout i vysvětlit**. Chybová hláška „neplatná hodnota" vede operátory
k obcházení (odtud zástupné hodnoty a stínové taxonomie). Hláška s důvodem a nabídkou
správné hodnoty ne.

## 4. Monitoring

Metriky z `dq-validator` spouštěj periodicky nad stejným předpisem a ukládej **časovou řadu**,
ne jen aktuální hodnotu.

```sql
CREATE TABLE DQ_HISTORY (
  MEASURED_AT datetime NOT NULL, TABLE_NAME varchar(64), COLUMN_NAME varchar(64),
  DIMENSION varchar(20), SCORE decimal(6,4), ROWS_TOTAL bigint, ROWS_FAILED bigint,
  PRIMARY KEY (MEASURED_AT, TABLE_NAME, COLUMN_NAME, DIMENSION));
```

Alertuj na **změnu**, ne na absolutní hodnotu. Úplnost 92 % může být normál; pokles z 99 % na
92 % za den je incident. Prahy: skokový pokles skóre, nová hodnota mimo číselník, nárůst
podílu zástupných hodnot, propad match rate na registr, objem mimo očekávaný rozsah.

Na „změnu" existují míry, které obhájíš líp než práh od oka: **PSI** (population stability
index) a **Kullback-Leiblerova divergence** na posun rozdělení hodnot, **Kolmogorov-Smirnovův
test** na shodu s referenčním obdobím. Na stabilitu v čase se hodí **Shewhartův regulační
diagram** — střední hodnota, pásma ±1/2/3 sigma, LCL a UCL; odliší běžný šum od signálu líp
než pevná mez.

Posunu rozdělení hodnot se říká **data drift**, změně struktury **schema drift**. Z toho plyne,
že profiling není jednorázová příprava před auditem: má běžet **při každém loadu**, ne jednou
za projekt.

Rozdíl proti data observability: monitoring měří **kvalitu obsahu** podle definovaných pravidel,
observability sleduje **chování pipeline** — pět pilířů: čerstvost, rozdělení hodnot, objem,
schéma a lineage. Potřebuješ obojí; schema drift totiž tiše rozbije samotná pravidla.

Metadatový repozitář (`DQM_MDR`) drž aktuální — je to zároveň konfigurace monitoringu
i dokumentace. Když nepokrývá celé univerzum, monitoring má slepá místa.

## 5. Data governance

Technická opatření vydrží, dokud za ně někdo odpovídá. Bez toho se to po roce vrátí do
původního stavu:

| Prvek | Obsah |
|---|---|
| vlastnictví dat | za každou doménu odpovídá konkrétní útvar, ne „IT" |
| data steward | jmenovitá role za kvalitu konkrétních atributů, s pravomocí měnit pravidla |
| datový katalog | co atribut znamená, odkud přichází, kdo ho mění, jaká pravidla platí |
| lineage a provenience | odkud hodnota přišla a čím prošla — bez toho nelze dohledat příčinu; na úroveň sloupců se lineage získává parsováním samotného SQL (SQLGlot), u nestandardních dialektů s doladěním gramatiky |
| správa číselníků | proces zavedení nové hodnoty, jinak vznikne stínová taxonomie |
| pravidelný re-audit | metriky se hlásí a řeší, ne jen měří |

`DQM_MDR` je minimální lokální varianta katalogu, dost na audit jednoho schématu. Cílový stav
governance je **federovaný datový katalog** (DataHub, Atlan, Collibra), který spojuje technická,
provozní i byznysová metadata — lineage, data contracts, business glossary — napříč zdroji.
Nevydávej tabulku v MySQL za datový katalog; je to jeho zárodek.

Prevence není jen kontrola na vstupním formuláři. Životní cyklus informace **POSMAD** (plan,
obtain, store, share, maintain, apply, dispose) ukazuje, kde všude defekt vzniká — sdílení,
údržba i likvidace jsou stejně platná místa jako pořízení. Projdi cyklus krok po kroku a ke
každému se zeptej, jaká kontrola tam chybí.

Chybějící kategorie v číselníku (příklad: podnikající fyzická osoba, která má současně osobní
i firemní identifikátor) je governance problém, ne datový. Dokud nikdo nesmí číselník rozšířit,
budou operátoři plnit nejbližší nesprávnou hodnotu.

## Výstup strážce

- DDL skript: typy, FK, UNIQUE, CHECK — v pořadí, v jakém projde.
- Katalog pravidel firewallu, každé s odkazem na nález, který ho vyvolal.
- Definice monitorovaných metrik, prahů a příjemců alertů.
- Návrh governance: role, vlastnictví, proces změny číselníku.
- **Seznam opatření, která nešla nasadit, a co je blokuje** — vstup do dalšího kola.

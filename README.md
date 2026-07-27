# dq-skills

Sada Claude Code skillů pro **audit a čištění kvality dat v libovolné databázi**.

Deset navazujících skillů: od profilingu neznámé databáze přes měření DQ metrik a zprávu
auditora až po standardizaci, imputaci, deduplikaci a prevenci opakování chyb.

Metodicky vychází z kurzu 4IZ562 Řízení kvality dat (VŠE) a z reálného auditu 23 tabulek
pojišťovny — 64 zjištění, kvantifikace COPQ a implementace nápravných opatření. Skilly jsou
ale psané doménově nezávisle: vzorce defektů a postupy platí stejně pro e-shop, nemocnici
nebo státní registr.

## Pipeline

```
dq-profiler → dq-validator → dq-auditor                    audit
     ↓
dq-parser → dq-standardizator → dq-adresar → dq-imputator → dq-deduplikator → dq-strazce
                                                            remediace
```

| Skill | Co dělá |
|---|---|
| **dq-audit** | vstupní bod: DQ dimenze, formát zjištění, přenositelnost SQL i doménových pravidel |
| **dq-profiler** | inventura, struktura, collation, PK/FK, distribuce, katalog vzorců defektů |
| **dq-validator** | kontroly po šesti dimenzích, checksum validátory, zápis skóre do metadatového repozitáře |
| **dq-auditor** | katalog zjištění, root-cause, COPQ a ROI, legislativní kontext, prioritizace |
| **dq-parser** | atomizace složených hodnot (číslo domu z ulice, obec vs. městská část, jméno) |
| **dq-standardizator** | kanonický tvar, napojení na referenční slovníky, display vs. match key |
| **dq-adresar** | napojení na adresní registr, match code, hierarchie přesnosti, match rate |
| **dq-imputator** | doplňování chybějících hodnot podle spolehlivosti zdroje — a kdy neimputovat |
| **dq-deduplikator** | match code, klastry, survivor, golden record, household |
| **dq-strazce** | datové typy, FK/CHECK, DQ firewall, monitoring, data governance |

Pořadí není libovolné: deduplikace před standardizací nesloučí `MuDr` a `MUDr.`, constrainty
před remediací neprojdou.

## Instalace

```bash
git clone https://github.com/koprjaa/dq-skills.git
cp -R dq-skills/skills/* ~/.claude/skills/
```

Projektová instalace (jen pro jeden repozitář): kopíruj do `.claude/skills/` v projektu.

## Použití

Skilly se aktivují samy podle popisu. Stačí napsat, co chceš:

```
zprofiluj mi tuhle databázi
změř kvalitu dat v tabulce zákazníků
napiš zprávu auditora z toho, co jsi našel
vyčisti a zdeduplikuj klientský kmen
```

Nebo explicitně: `/dq-profiler`, `/dq-validator`, …

Když nevíš, kterým krokem začít, spusť `dq-audit` — je to mapa pipeline a společný standard.

## Podporované databáze

SQL v katalozích je psané pro MySQL; `dq-audit` obsahuje překladovou tabulku pro PostgreSQL,
SQL Server, Oracle, SQLite a DuckDB (regex, metadata, hash, podmíněné agregace, collation).

Doménové validátory (rodné číslo, IČO, PSČ, RÚIAN, CZ-NACE) jsou příkladem pro ČR — struktura
kontroly zůstává, konkrétní pravidla se nahrazují ekvivalentem jurisdikce podle mapovací
tabulky v `dq-audit`.

## Zásady, na kterých to stojí

- Žádné tvrzení o kvalitě bez dotazu a bez čísla.
- Ke každému číslu univerzum — „5 328 chybí" nic neznamená.
- Originální sloupec se nikdy nepřepisuje; remediace jde do `_STD` sloupců.
- Křížová kontrola dělá z podezření důkaz.
- Oprava dat bez opravy vstupní kontroly znamená, že se defekt vrátí.

## Licence

MIT

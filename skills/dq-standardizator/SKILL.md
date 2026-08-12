---
name: dq-standardizator
description: Standardizace dat do kanonického tvaru a napojení na referenční slovníky — tituly, jména a příjmení proti registru, rodné číslo a IČO, PSČ, e-mail, telefon, diakritika, case, padding, zástupné hodnoty na NULL, konvence _STD sloupců a match klíčů. Druhý krok remediace, po dq-parser, před dq-imputator. Použij, když je potřeba "sjednotit zápis", "standardizovat hodnoty", "napojit na číselník", "normalizovat jména nebo tituly", "vyčistit e-maily a telefony". Keywords: standardizace, unifikace, normalizace, kanonický tvar, referenční slovník, diakritika, match key, _STD sloupce, čištění hodnot.
---

# Standardizátor — kanonický tvar

Druhý krok remediace. Data už jsou atomická (`dq-parser`). Teď každý atribut dostane **jeden
kanonický tvar** a napojení na referenční slovník. Bez toho neproběhne ani imputace, ani
deduplikace: `MuDr` a `MUDr.` jsou pro stroj dvě různé hodnoty.

Pipeline: `dq-parser` → **dq-standardizator** → `dq-adresar` → `dq-imputator` → `dq-deduplikator`.

## Konvence

- **Nový sloupec `<ATRIBUT>_STD`, originál se nemění.** Před/po je evidence do zprávy a
  jediná cesta zpět, když se pravidlo ukáže jako špatné.
- **Každá oprava je zaregistrované pravidlo**, ne ad-hoc `UPDATE`. Pravidlo má jméno, popis
  vzoru, počet zasažených řádků a odkaz do metadatového repozitáře. Sem patří i deterministické
  opravy, které vypadají triviálně (záměna znaku z migrace) — `dq-parser` je proto neopravuje,
  jen označí. Bez registru pravidel nedohledáš, čím se hodnota mezi originálem a `_STD` změnila.

```sql
ALTER TABLE PART_PARTY
  ADD PARTY_RC_STD      varchar(10), ADD PARTY_FNAME_STD varchar(45),
  ADD PARTY_MNAME_STD   varchar(45), ADD PARTY_LNAME_STD varchar(45),
  ADD PARTY_TITBEF_STD  varchar(10), ADD PARTY_TITAFT_STD varchar(10);
```

- **Dva různé výstupy z jedné hodnoty**, nepleť si je. Kurz 4IZ562 pro ně má dvě různá jména
  a je to podstatný rozdíl, ne slovíčkaření:

| Výstup | Kurz tomu říká | K čemu | Jak vypadá |
|---|---|---|---|
| **display** (`_STD`) | **standardizace** | zobrazení, korespondence, reporting, export | `Nováková`, `MUDr.`, `Praha 5` |
| **match key** (`_MATCH`) | **unifikace** | porovnávání, dedup, JOIN na registr | `NOVAKOVA`, `mudr`, `praha5` |

Standardizace zvyšuje kvalitu hodnoty pro člověka a pro reporting. Unifikace hodnotu naopak
**zplošťuje**, aby ji mohl porovnat algoritmus — a sama o sobě kvalitu nezvyšuje. Když v textu
napíšeš „standardizace" tam, kde jde o klastrování duplicit, je to metodická chyba.

Match key (unifikovaný tvar): bez diakritiky, uppercase (nebo lowercase — hlavně jednotně),
bez interpunkce a mezer. Display si diakritiku a interpunkci **ponechává** — jinak vyrobíš
korespondenci s „Novakova".

- **Standardizace nesmí měnit match key**, jinak se přerovnají klastry v deduplikaci.
  Oprava diakritiky `MARTÍNKOVA` → `MARTÍNKOVÁ` mění display, match key zůstává `MARTINKOVA`.

## Zástupné hodnoty → NULL (nejdřív ze všeho)

Zástupná hodnota není data. Než začneš standardizovat, převeď ji na NULL — jinak se
`9999999999` stane „validním rodným číslem" u tisíců klientů a `NA` se stane příjmením.

```sql
UPDATE PART_PARTY SET PARTY_RC = NULL
WHERE TRIM(LOWER(COALESCE(PARTY_RC,''))) IN
      ('','cizinec','nevyplneno','nevyplněno','9999999999','0000000000');

UPDATE PART_PARTY SET PARTY_FNAME = NULL, PARTY_LNAME = NULL
WHERE PARTY_TYPE='C' AND TRIM(UPPER(COALESCE(PARTY_FNAME,''))) IN ('NA','N/A','NULL','.');
```

Odborně jde o **maskovanou neúplnost** (disguised missing data): hodnota v poli je, ale data
za ní nejsou, takže profiling vykázal falešně vysokou úplnost.

Spočítej a zapiš, kolik jich bylo — je to before/after evidence a zároveň korekce metriky
úplnosti (úplnost po převodu klesne, a to je správně: dřív byla falešně nadhodnocená).

## Rodné číslo

```sql
UPDATE PART_PARTY SET PARTY_RC_STD = REPLACE(TRIM(PARTY_RC),'/','')
WHERE PARTY_TYPE='P' AND REPLACE(TRIM(PARTY_RC),'/','') REGEXP '^[0-9]{9,10}$';
```

Vedle `_STD` drž **dva příznaky**, ne jeden — je to nejdůležitější rozhodnutí v celé standardizaci:

| Příznak | Význam | K čemu |
|---|---|---|
| `RC_USABLE` | struktura čitelná (měsíc 1–12 po odečtu 50/20/70, den 1–31) | interní odvození klíče pro dedup |
| `RC_VALID` | prošlo i checksum mod 11 | **výstup** — publikovaná hodnota, odvozené datum a pohlaví |

Do exportu a do odvozených atributů pouštěj jen `RC_VALID`. `RC_USABLE` slouží jen jako
dedup signál uvnitř pipeline — čitelné, ale checksum-rozbité rodné číslo pořád spolehlivě
identifikuje tutéž osobu ve dvou záznamech, ale publikovat se nesmí.

Dvojice příznaků je moje rozšíření: kurz 4IZ562 do `_STD` ukládá rovnou jen plně validní
hodnoty. Rozpor to není — nevalidní hodnota se do `_STD` nedostane ani tady, jen si vedle
držím informaci, že je *čitelná*, protože bez ní přijdeš o část dedup signálu.

## IČO

```sql
-- doplň vedoucí nuly na 8, nečíselné zahoď
UPDATE PART_PARTY SET PARTY_CREGNUM_STD = LPAD(REGEXP_REPLACE(PARTY_CREGNUM,'[^0-9]',''),8,'0')
WHERE PARTY_TYPE='C' AND LENGTH(REGEXP_REPLACE(PARTY_CREGNUM,'[^0-9]','')) BETWEEN 1 AND 8;
```

Nikdy neukládej IČO do číselného typu — přijdeš o vedoucí nuly a `00123456` se stane `123456`.
Správný typ je `char(8)`: délka je garantovaná, takže je to výjimka z pravidla „char nahradit
varcharem" (`dq-strazce`) — to platí na proměnlivě dlouhý text, ne na kód s pevnou délkou.

Doplnění nul na osm znaků není kosmetika: kratší IČO mají třeba organizační složky státu
a bez zarovnání se na registr nenapojí.

## Tituly — napojení na referenční slovník

Vzor, který funguje: **normalizuj obě strany stejně** (lower + odstranění teček + trim) a
napoj přes normalizovaný tvar, ale ulož **kanonickou hodnotu z registru**.

```sql
UPDATE PART_PARTY a
INNER JOIN REF_TITBEF b
  ON LOWER(REPLACE(TRIM(CONVERT(a.PARTY_TITBEF USING utf8mb4)),'.','')) =
     LOWER(REPLACE(TRIM(CONVERT(b.VALUE        USING utf8mb4)),'.',''))
SET a.PARTY_TITBEF_STD = b.VALUE;
```

Tímhle jediným krokem se shoda s číselníkem posune z ~21–29 % na ~81 %. Zbytek jsou reálné
problémy, ne varianty zápisu, a rozdělují se na tři koše:

| Koš | Příklad | Co s tím |
|---|---|---|
| starý/regionální zápis | `DrS` vs `DrSc.`, `PhD` vs `Ph.D.` | ruční mapovací tabulka historických tvarů |
| sémantický šum | `otec` v poli titulu (2 118 řádků) | na NULL, plus finding — pole se používá k jinému účelu |
| chybí v registru | tvar, který je legitimní, ale slovník ho nezná | doplnit do registru, ne ohýbat data |

Pozor na registr samotný: může obsahovat duplicitní hodnotu pod dvěma kódy (`Dipl.tech.` pod
CODE 19 i 20) nebo překlep v popisu (`Bacherol`). Pak je napojení nejednoznačné — sluč kódy
v registru dřív, než na něj napojíš provoz.

## Jména a příjmení

Trojice kroků v tomhle pořadí:

**1. Basic clean + display/match rozdvojení**

```python
display = " ".join(w.capitalize() for w in s.split())
match   = re.sub(r"[^A-Z0-9 ]", "", unidecode(s).upper()).strip()
```

**2. Registr-based recovery ztraceného písmene.** Poškozené příjmení (`BE0ŠOVÁ`) rekonstruuj
proti registru jmen: `0` nahraď jokerem `.{1,2}` a hledej shodu; při více kandidátech vezmi
ten s nejvyšší četností. Když registr nezná diakritickou variantu, zkus fallback bez diakritiky.

```python
pat  = "^" + "".join(".{1,2}" if ch == "0" else re.escape(ch) for ch in disp.upper()) + "$"
cand = ref[ref["U"].str.fullmatch(pat, na=False)]
if len(cand) == 0:                                   # fallback bez diakritiky
    patA = "^" + "".join(".{1,2}" if ch == "0" else re.escape(ch) for ch in unidecode(disp.upper())) + "$"
    cand = ref[ref["UASC"].str.fullmatch(patA, na=False)]
best = cand.sort_values("FREQ", ascending=False).iloc[0]["VALUE"] if len(cand) else None
```

**3. Standardizace display na registr** (oprava diakritiky a fonetiky: `MARTÍNKOVA` →
`MARTÍNKOVÁ`, `TÚMOVÁ` → `TŮMOVÁ`). Mapuj přes accent-insensitive klíč na nejčetnější variantu
v registru. Match key se **nemění**, takže dedup zůstává beze změny.

Deterministická přechýlená koncovka se opravuje **před** registrem — registr obsahuje i
nediakritické junk tvary, takže exact match by vrátil zase špatnou variantu:

```python
if is_lname and disp.endswith("ova"):
    disp = disp[:-1] + "á"
```

Co registr neopraví, zůstává poškozené s příznakem — sloučí to až `dq-deduplikator`
z čistého dvojníka téže osoby v klastru.

## E-mail

```python
s = str(v).strip().lower()
s = s.replace("#", "@").replace("&", ".")     # systematická záměna z importu
s = re.sub(r"\s+", "", s)                     # mezery uvnitř adresy
valid = bool(re.fullmatch(r"[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,}", s))
```

Ta dvě `replace` jsou legitimní jen proto, že jde o **prokázaně systematickou** záměnu
(22 694 řádků jednoho vzoru), ne o překlepy. Ověř to nejdřív četností, jinak si vyrobíš
falešně platné adresy. Do exportu pouštěj jen `valid`.

## Telefon

```python
d = re.sub(r"\D", "", str(v))
d = re.sub(r"^00420", "", d)
d = re.sub(r"^420", "", d) if len(d) > 9 else d
if len(d) != 9 or d == d[0] * 9:   return (None, None)     # 999999999 = bogus
if d[0] in ("6", "7"):             return (d, None)        # mobil
if d[0] in ("2", "3", "4", "5"):   return (None, d)        # pevná
```

Rozdělení mobil/pevná není kosmetika: pevná linka vedená jako mobil je důvod, proč SMS kanál
tiše nefunguje. Standardizace je zároveň **reklasifikace typu kontaktu**.

## PSČ

```sql
UPDATE PARTY_ADDRESS SET ADDR_ZIP_STD = REGEXP_REPLACE(ADDR_ZIP,'[^0-9]','')
WHERE REGEXP_REPLACE(ADDR_ZIP,'[^0-9]','') REGEXP '^[0-9]{5}$';
```

Useknuté PSČ (`251 6` z původního `251 62`) **nedoplňuj hádáním** — chybí poslední číslice.
Řeší se dohledáním v registru přes obec + ulici (`dq-imputator`), ne dopočtem.

Ukládej jako `varchar`, ne `integer` — číselný typ zabije vedoucí nulu a zahraniční
alfanumerické kódy.

## Padding a case

```sql
UPDATE <t> SET <col> = NULLIF(TRIM(<col>), '');
```

Padding z `char(n)` odstraň globálně jako první, jinak každé pozdější porovnání potřebuje
`TRIM` a někde se na něj zapomene. Zároveň navrhni změnu typu na `varchar` — jinak se padding
vrátí při příštím zápisu (patří do `dq-strazce`).

## Výstup standardizátoru

- `_STD` sloupce, `_MATCH` klíče, příznaky validity (`RC_VALID`, `EMAIL_VALID`, `_CORRUPT`).
- **Before/after tabulka na každý atribut**: kolik hodnot se změnilo, o kolik vzrostla shoda
  s registrem, kolik zůstalo mimo a proč.
- Seznam hodnot, které standardizace nevyřešila — vstup pro rozšíření registru nebo pro
  ruční review.

---
name: dq-parser
description: Parsing a atomizace dat — rozbití složených hodnot do dedikovaných sloupců (číslo popisné a orientační z ulice, obec vs. městská část, jméno a prostřední jméno, titul od jména), oprava prokládaných mezer a kódovacích chyb, extrakce vzorů regexem. První krok remediace, po auditu, před dq-standardizator. Použij, když jsou "čísla domů slitá v ulici", "složené hodnoty v jednom poli", "porušená atomicita", "je potřeba rozparsovat adresu nebo jméno". Keywords: parsing, atomizace, atomicita, rozdělení adresy, číslo popisné, regexp_substr, split, extrakce.
---

# Parser — atomizace

První krok remediace. **Nic nestandardizuj ani neunifikuj, dokud to není atomické.** Pole
`ADDR_STREET` s hodnotou „Na Budíně 854" nenapojíš na registr, který zná ulici a číslo zvlášť.

Porušená atomicita není kosmetika, je to **porušení první normální formy**: bez atomických
hodnot nemá model spolehlivý identifikátor záznamu a jakékoli přibližné porovnávání
a slučování s dalším zdrojem je nespolehlivé.

Pipeline: audit → **dq-parser** → `dq-standardizator` → `dq-adresar` → `dq-imputator` → `dq-deduplikator`.

## Pravidla

- **Originál se nemaže.** Parsovaný výstup jde do nových sloupců (`_STD`, `_NUM1`, `_NUM2`).
  Original je evidence pro before/after a záchranná síť, když parser šlápne vedle.
- **Deterministicky, ne heuristikou.** Parser, který v 3 % případů uhodne špatně, vyrobí
  tichá poškození. Když si pravidlem nejsi jistý, nech NULL a spočítej, kolik jich zbylo.
  Platí pro strukturované pole se známým vzorem — adresa, jméno, datum. U volného textu,
  kde vzor neexistuje, je statistické rozpoznávání entit (NER, spaCy) legitimní nástroj,
  ale jeho výstup je odhad: patří k němu skóre jistoty a příznak, a do produkčního sloupce
  nejde bez revize.
- **Osekej nepřípustné znaky dřív, než začneš dělit.** Pro každý sémantický typ si drž tabulku
  přípustných znaků (*chop table*) — u poštovního kódu číslice, u IBANu alfanumerika — a zbytek
  odstraň. Úspěšnost rozkladu to zvedne víc než složitější regex.
- **Změř míru rozparsování** — kolik řádků se povedlo rozložit. To je metrika do zprávy
  a nezaměňuj ji s *match rate*: ten se měří až při napojení na registr (`dq-adresar`).
- Parsuj **před** čímkoli dalším: standardizace, imputace i deduplikace stojí na atomických polích.

Regex řeší jeden vzor. Průmyslové parsery jdou na to přes **gramatiku** sémantického typu
uloženou ve znalostní bázi (QKB): popíše očekávanou stavbu adresy nebo jména a jednotlivým
kombinacím tokenů přiřadí pravděpodobnost výskytu. Když píšeš pátý regex na tutéž entitu,
chybí ti gramatika, ne další regex.

## Adresa

Nejčastější defekt: dedikované sloupce pro čísla domů jsou 100 % prázdné a číslo je slité do
názvu ulice. Ve zdroji vypadá jako `Na Budíně 854`, `Nádražní 1211/25a`, `Bezručova 12/3`.

Rozklad na tři složky — název ulice, číslo popisné, číslo orientační (s případným písmenem):

```sql
-- MySQL 8: REGEXP_SUBSTR / REGEXP_REPLACE
UPDATE PARTY_ADDRESS SET
  ADDR_NUM1_STD   = REGEXP_SUBSTR(ADDR_STREET, '[0-9]+(?=(/[0-9]+[a-zA-Z]?)?[[:space:]]*$)'),
  ADDR_NUM2_STD   = REGEXP_SUBSTR(ADDR_STREET, '(?<=/)[0-9]+[a-zA-Z]?[[:space:]]*$'),
  ADDR_STREET_STD = TRIM(REGEXP_REPLACE(ADDR_STREET, '[0-9]+(/[0-9]+[a-zA-Z]?)?[[:space:]]*$', ''));
```

Python ekvivalent, když jde o pandas pipeline:

```python
num  = raw.str.extract(r'(\d+)(?:/\d+\w?)?\s*$', expand=False)      # číslo popisné
street = raw.str.replace(r'\d+(/\d+\w?)?\s*$', '', regex=True).str.strip()
```

Pozor na pořadí čísel: v ČR se zapisuje `popisné/orientační`, ale konvence není vždy dodržená.
Když parser najde jen jedno číslo, ulož ho jako popisné a orientační nech NULL — nehádej.

### Složená hodnota v poli obce

`Praha 5 - Smíchov`, `České Budějovice - České Budějovice 5` — operátor napsal do jednoho pole
celou adresní hierarchii (obec + městská část + číslo obvodu). Znemožňuje párování na registr.

```sql
-- první segment před pomlčkou = obec, zbytek = městská část
SELECT ADDR_CITY,
  TRIM(SUBSTRING_INDEX(ADDR_CITY, '-', 1))                      obec,
  NULLIF(TRIM(SUBSTRING(ADDR_CITY, LOCATE('-',ADDR_CITY)+1)),'') mcast
FROM PARTY_ADDRESS WHERE ADDR_CITY LIKE '%-%';
```

Detekce kandidátů, než začneš parsovat:

```sql
SELECT ADDR_CITY, COUNT(*) cnt FROM PARTY_ADDRESS
WHERE ADDR_CITY REGEXP '[0-9]' OR ADDR_CITY LIKE '%-%' OR LENGTH(ADDR_CITY) > 50
GROUP BY 1 ORDER BY cnt DESC LIMIT 20;
```

Pozor: pomlčka v názvu obce je někdy legitimní (`Frýdek-Místek`, `Brandýs nad Labem-Stará
Boleslav`). Proto se rozklad ověřuje proti registru obcí, ne slepě — a když po rozkladu obec
v registru není, vrať původní hodnotu.

### P.O. Box a jiné mimo-rozsahové formy

P.O. Box, poštovní přihrádka, „na doručovací adrese firmy" — formálně platné adresy, které
registr budov nezná. Neparsuj je násilím, označ je vlastním příznakem a vyřaď z metriky
match rate (jinak vykážeš jako chybu něco, co chyba není).

## Jméno

### Prokládané mezery

`D A V I D`, `Ž A N N A` — každé písmeno oddělené mezerou. Slepit lze deterministicky, když
jsou **všechny** tokeny jednopísmenné, nebo když je jednopísmenných většina (zbytek je typicky
pomlčka nebo iniciála):

```python
toks = s.split()
if len(toks) > 1 and all(len(t) == 1 for t in toks):
    s = "".join(toks)
elif len(toks) > 1 and sum(len(t) == 1 for t in toks) >= max(2, len(toks) - 1):
    s = "".join(toks)
```

Nikdy neslepuj tokeny obecně — `Anna Marie` je dvě jména, ne `AnnaMarie`.

### Vícesložkové jméno

Křestní + prostřední jméno v jednom poli. Rozděl podle mezery a druhý token ulož zvlášť;
jednopísmenné tokeny (iniciály) zahazuj, jinak vyrobíš nesmyslné prostřední jméno:

```sql
UPDATE PART_PARTY
SET PARTY_FNAME_STD = <proper>(SUBSTRING_INDEX(PARTY_FNAME,' ',1)),
    PARTY_MNAME_STD = NULLIF(<proper>(SUBSTRING_INDEX(SUBSTRING_INDEX(PARTY_FNAME,' ',2),' ',-1)),
                             <proper>(SUBSTRING_INDEX(PARTY_FNAME,' ',1)))
WHERE PARTY_TYPE='P' AND LENGTH(SUBSTRING_INDEX(PARTY_FNAME,' ',2)) > 1;
```

### Titul zapsaný v poli jména

`Ing. Jan Novák` v `PARTY_NAME`. Detekuj podle shody prvního tokenu se slovníkem titulů,
ne podle tečky — `J. Novák` je iniciála, ne titul.

Slovníkem je číselník titulů (`REF_*_TITBEF` pro tituly před jménem, `TITAFT` za ním) a shodu
dělej fuzzy, protože v datech vedle sebe stojí `MuDr`, `MUDr.` i `Mudr`. Parser titul jen
oddělí; převod na kanonický tvar je práce pro `dq-standardizator`.

### Kódovací chyby

Dvě různé věci, dvě různá řešení:

| Chyba | Příklad | Řešení |
|---|---|---|
| systematická záměna znaku | `ĄUDOVÍT` (má být `ĽUDOVÍT`) | vzor je jednoznačný (`Ą` na začátku slova je vždy chybný přepis), ale oprava patří do `dq-standardizator` jako registrované pravidlo. V parseru jen tehdy, když ten znak brání samotnému rozkladu |
| ztracené písmeno | `BE0ŠOVÁ` (má být `BENEŠOVÁ`) | **neopravuj v parseru.** Označ příznakem `_CORRUPT` a nech obnovu na `dq-standardizator` (registr jmen) nebo `dq-deduplikator` (čistý dvojník v klastru) |

```python
s = re.sub(r"(^|(?<=\s))Ą", "Ľ", s)          # jednoznačné pravidlo — ale patří do standardizace
corrupt = bool(re.search(r"[0-9]", s))       # číslice ve jménu = poškozeno, řeší se jinde
```

**Parser dělí, neopravuje.** I deterministická oprava patří do standardizace jako zaregistrované
pravidlo — jinak po ní nezůstane auditní stopa a nikdo nedohledá, čím se hodnota změnila.
Jediná výjimka je znak, který brání samotnému rozkladu. Rekonstrukce chybějícího písmene je
navíc odhad a patří tam, kde je proti čemu ho ověřit.

## Datum uložené jako text

Než parsuješ, ověř formát a den/měsíc:

```sql
SELECT SUBSTRING(<col>,1,2) prvni, SUBSTRING(<col>,4,2) druhy, COUNT(*)
FROM <t> GROUP BY 1,2 ORDER BY 3 DESC LIMIT 20;
```

Když první pozice nikdy nepřekročí 12 a druhá ano, je pořadí `MM.DD`; když obojí přesahuje 12,
je formát smíšený a je nutné rozhodnout per řádek podle druhého zdroje. Slepý `STR_TO_DATE`
tichý swap nezachytí a rozseje ho po celé tabulce.

## Výstup parseru

- Nové `_STD` sloupce s atomickými hodnotami, originály nedotčené.
- Míra rozparsování na každou složku (`% řádků s neprázdným výsledkem`).
- **Chybová tabulka** řádků, které parser nerozložil, s důvodem u každého — auditní stopa
  o tom, proč záznam neprošel. Vstup pro ruční review nebo pro rozhodnutí „tohle je mimo
  rozsah".

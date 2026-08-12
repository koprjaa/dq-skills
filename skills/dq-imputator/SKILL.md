---
name: dq-imputator
description: Doplňování chybějících hodnot — odvození z jiného atributu téhož záznamu, agregace z navázaných tabulek, lookup do referenčního registru, defaultní kategorie, statistická a modelová imputace, a hlavně pravidla kdy neimputovat. Krok remediace po dq-adresar, před dq-deduplikator. Použij, když se mají "doplnit chybějící hodnoty", "dopočítat datum nebo pohlaví", "imputace", "obohatit data z registru", "co s NULL hodnotami". Keywords: imputace, doplnění chybějících hodnot, odvození, derived column, lookup, obohacení dat, enrichment, missing values, MCAR, MAR, MNAR, mechanismus výskytu, Buckova metoda, hot deck.
---

# Imputátor — doplnění chybějících hodnot

Doplň jen to, co jde **odvodit s jistotou** nebo dohledat v autoritativním zdroji. Odhad
uložený do produkční tabulky vypadá pak stejně jako fakt — a nikdo už nepozná rozdíl.

Pipeline: `dq-adresar` → **dq-imputator** → `dq-deduplikator`.

## Pořadí spolehlivosti

| # | Metoda | Spolehlivost | Použití |
|---|---|---|---|
| 1 | **odvození z jiného atributu téhož záznamu** | deterministická | identifikátor nese datum, pohlaví, kontrolní číslici |
| 2 | **agregace z navázané tabulky** | deterministická | datum vzniku vztahu = minimum z transakcí |
| 3 | **lookup do referenčního registru** | vysoká | sektor, klasifikace odvětví, právní forma, obec podle kódu adresy |
| 4 | **defaultní kategorie pro celou třídu entit** | metodická | „fyzická osoba patří do sektoru domácnosti" |
| 5 | **bez modelu** (průměr, medián, modus, midrange) | nízká | deformuje rozdělení, viz níže |
| 6 | **implicitní model** (Hot Deck, Cold Deck, k-NN) | střední, ale dohledatelná | analytická vrstva, s příznakem |
| 7 | **explicitní model** (Buckova metoda, logistická regrese, Naive Bayes, stromy, MCMC) | nejnižší | modelování a výzkum |

Metody 1–4 (databázové techniky, odvození z ostatních hodnot) jdou do produkce bez diskuse.
Výš rozhoduje **vysvětlitelnost, ne to, jestli je metoda statistická**:

- Robustní a auditovatelná metoda — Buckova (podmíněný průměr z regrese), Hot Deck — se do
  provozu dostat může, s příznakem a s popsaným postupem.
- Black box (AutoML, k-NN nad desítkami proměnných, neuronová síť) ne. Když instituce zamítne
  klientovi službu na základě skóringu, musí umět vysvětlit, odkud se vzala každá vstupní
  hodnota. Co neobhájíš před regulátorem, do provozu nepatří.

## Železná pravidla

- **Nikdy neimputuj přes nevalidní zdroj.** Odvozuj jen z hodnoty, která prošla validací
  (checksum, ne jen formát). Datum narození spočítané z rodného čísla s rozbitým checksumem je
  vymyšlené datum s tváří faktu.
- **Nikdy nepřepiš vyplněnou hodnotu.** Imputace plní jen NULL. Kolize (uložená hodnota se liší
  od odvozené) je *nález pro audit* — defekt vnitřní konzistentnosti — ne důvod k přepsání.
  Který ze dvou zdrojů je věrohodnější, se rozhoduje mimo imputátor a s odůvodněním.
- **Vždy do `_STD` sloupce**, nikdy do originálu. Původní NULL musí zůstat viditelné.
- **Označ původ.** Buď zvláštní příznak (`_IMPUTED`), nebo oddělený sloupec. Bez toho nelze
  spočítat, kolik dat je naměřených a kolik dopočtených.
- **Sanity check po imputaci.** Rozsah, distribuce, extrémy — a nejen technický rozsah typu,
  ale **byznysové omezení**: datum narození ne v budoucnosti, věk pojistníka nad hranicí
  svéprávnosti, datum smlouvy v době existence produktu. Odvození roku z dvojčíslí bez
  správného století umí vyrobit klienty narozené v roce 2074.
- **Změř před/po.** Úplnost atributu před imputací, po imputaci, a kolik zbývá — to je hlavní
  výstup tohoto kroku.

## 1. Odvození z jiného atributu

Identifikátor osoby často nese datum narození a pohlaví. Vzor (české rodné číslo; v jiné doméně
nahraď ekvivalentním identifikátorem):

```sql
ALTER TABLE PART_PARTY ADD PARTY_DOFBIRTH_STD date NULL, ADD PARTY_GENDER_STD char(1) NULL;

-- 1) přenes existující hodnoty
UPDATE PART_PARTY SET PARTY_DOFBIRTH_STD = PARTY_DOFBIRTH WHERE PARTY_DOFBIRTH IS NOT NULL;

-- 2) doplň jen tam, kde chybí A zdroj je validní
UPDATE PART_PARTY SET PARTY_DOFBIRTH_STD = STR_TO_DATE(CONCAT(
    CASE WHEN LENGTH(TRIM(PARTY_RC))=10 AND SUBSTR(PARTY_RC,1,2)*1 < <pivot>
         THEN CONCAT('20',SUBSTR(PARTY_RC,1,2)) ELSE CONCAT('19',SUBSTR(PARTY_RC,1,2)) END,'-',
    CASE WHEN SUBSTR(PARTY_RC,3,2)*1 > 12 THEN SUBSTR(PARTY_RC,3,2)*1 - 50
         ELSE SUBSTR(PARTY_RC,3,2)*1 END,'-', SUBSTR(PARTY_RC,5,2)), '%Y-%m-%d')
WHERE PARTY_TYPE='P' AND PARTY_DOFBIRTH_STD IS NULL AND RC_VALID = 1;

-- 3) sanity check: nesmyslné výsledky zpět na NULL
UPDATE PART_PARTY SET PARTY_DOFBIRTH_STD = NULL WHERE YEAR(PARTY_DOFBIRTH_STD) < 1900;
```

Stoletý pivot je past. Devítimístná varianta je vždy 19xx; u desetimístné rozhoduj proti
snapshot datu (`2000+yy <= rok_snapshotu` → 2000s, jinak 1900s), ne proti pevné konstantě.

Pohlaví ze stejného zdroje:

```sql
UPDATE PART_PARTY
SET PARTY_GENDER_STD = CASE WHEN SUBSTR(PARTY_RC,3,2)*1 BETWEEN 51 AND 62 THEN 'F' ELSE 'M' END
WHERE PARTY_TYPE='P' AND PARTY_GENDER_STD IS NULL AND RC_VALID = 1;
```

**Zachovej původní hodnotu, doplň jen chybějící.** Když se uložené pohlaví liší od odvozeného,
je to nález pro `dq-auditor`, ne úkol pro imputátor.

## 2. Agregace z navázané tabulky

Datum vzniku vztahu = nejranější doklad vztahu. Ber minimum z obou zdrojů, ne jen fallback:

```sql
ALTER TABLE PART_PARTY ADD PARTY_SINCE_STD date NULL;
UPDATE PART_PARTY SET PARTY_SINCE_STD = PARTY_SINCE WHERE PARTY_SINCE IS NOT NULL;

UPDATE PART_PARTY a SET a.PARTY_SINCE_STD = LEAST(
    COALESCE(a.PARTY_SINCE_STD, '9999-12-31'),
    COALESCE((SELECT MIN(b.CNTR_VALIDFROM) FROM PROD_CONTRACT b WHERE b.PARTY_ID=a.PARTY_ID),
             '9999-12-31'))
WHERE a.PARTY_SINCE_STD IS NOT NULL OR EXISTS
      (SELECT 1 FROM PROD_CONTRACT b WHERE b.PARTY_ID = a.PARTY_ID);
UPDATE PART_PARTY SET PARTY_SINCE_STD = NULL WHERE PARTY_SINCE_STD = '9999-12-31';
```

Tím se zároveň opraví záznamy, kde bylo datum vztahu pozdější než první transakce — logický
nesmysl, který samotné doplnění NULL nevyřeší.

Pozor na sentinel data v agregaci: `MAX(valid_to)` přes `2999-12-31` vrátí sentinel. Filtruj
je **před** agregací, nebo je detekuj ze stringu — v pandas `Timestamp` končí rokem 2262 a
`2999` se tiše překlopí na `NaT`:

```python
yr = pd.to_numeric(contr["CNTR_VALIDTO"].astype(str).str[:4], errors="coerce")
open_ended = yr >= 2200                       # otevřená platnost, ne chybějící datum
vt = pd.to_datetime(contr["CNTR_VALIDTO"].where(~open_ended), errors="coerce")
```

## 3. Lookup do referenčního registru

Nejhodnotnější imputace: hodnota přichází z autoritativního externího zdroje.

```sql
-- klasifikace odvětví a právní forma podle identifikátoru firmy
UPDATE PART_PARTY a INNER JOIN REF_RES r ON r.ICO = TRIM(a.PARTY_CREGNUM)
SET a.PARTY_NACE_STD = r.NACE, a.PARTY_FORM_STD = r.ROSFORMA
WHERE a.PARTY_TYPE='C' AND a.PARTY_NACE_STD IS NULL;
```

Dvě věci k registrům přístupným přes API: mají **kvóty** (řádově tisíce dotazů denně, ve
špičce méně), takže dávkuj a cachuj, ať nedoběhneš do limitu uprostřed běhu. A ber hodnotu
**k datu, ke kterému ji potřebuješ** — sektor nebo právní forma z dneška nemusí platit pro
smlouvu z roku 2012.

Před napojením ověř **pokrytí registru** (viz `dq-validator`). Registr s 1 000 řádky pro
119 348 subjektů nedoplní nic užitečného a je to sám o sobě finding — správné doporučení
není „imputovat", ale „napojit se na skutečný registr".

Z kódu adresy (`dq-adresar`) lze zpětně dohledat chybějící PSČ nebo obec. Useknuté PSČ
(`251 6`) se **takhle** opravuje — dohledáním, ne dopočtem chybějící číslice.

## 4. Defaultní kategorie pro celou třídu entit

Když klasifikace má kategorii, do které daná třída entit patří definičně:

```sql
UPDATE PART_PARTY SET PARTY_ESA95_STD = 14000       -- Domácnosti
WHERE PARTY_TYPE='P' AND PARTY_ESA95_STD IS NULL;
```

To není odhad, ale metodické doplnění — a pořád patří do zprávy s odůvodněním. Rozdíl proti
plošné konstantě (viz `dq-profiler`) je v tom, že tady je hodnota **odvozená z typu entity**,
ne dosazená všem bez rozdílu. Ověř to: po imputaci nesmí mít 100 % záznamů stejnou hodnotu.

## 5. Statistická a modelová imputace

### Nejdřív mechanismus, pak metoda

Než sáhneš po statistice, urči **mechanismus výskytu chybějících hodnot**. Metodu vybírá on,
ne zvyk:

| Mechanismus | Co znamená | Důsledek |
|---|---|---|
| **MCAR** | chybí se stejnou pravděpodobností všude, nezávisle na čemkoli | statistická imputace je obhajitelná |
| **MAR** | pravděpodobnost chybění jde predikovat z ostatních proměnných | podmíněné metody (Buckova) dávají konzistentní odhad |
| **MNAR** | chybění závisí na hodnotě samotné (selektivní vyplňování) | statisticky neimputovat — je to nález o procesu |
| **MBND** | hodnota z podstaty existovat nemůže (počet těhotenství u muže) | neimputovat vůbec, opravit univerzum nebo model |

MCAR se **testuje, neodhaduje**: Littleův MCAR test, případně t-testy mezi skupinou s hodnotou
a bez ní. MAR proti MNAR bez dodatečné informace matematicky rozlišit nejde — a to je samo
o sobě důvod k opatrnosti.

Matice chybějících hodnot (`missingno`) je levný první pohled: systematická díra přes celou
třídu entit, jedno období nebo jeden kanál je na ní vidět hned.

### Dvě pasti

- **Deformace rozdělení.** Nahrazení chybějících hodnot jedinou konstantou (průměr, medián)
  vyrobí v histogramu nepřirozený pík uprostřed — v kurzu 4IZ562 *Čechové na Řípu*. Rozptyl
  klesne, korelace se rozjedou a závěry z takových dat neplatí, i když tabulka vypadá úplně.
- **Imputační paradox.** Model natrénovaný nad daty doplněnými explicitním modelem vykazuje
  *lepší* metriky (nižší MAE) než model nad původními úplnými daty. Není lepší — imputace do
  dat vnesla umělou multikolinearitu a přizpůsobila je. **Zlepšení metriky po imputaci ber
  jako varování, ne jako úspěch.**

## Co neimputovat nikdy

| Případ | Proč |
|---|---|
| identifikátor osoby nebo subjektu | vymyšlený identifikátor je horší než chybějící; blokuje ověřování a compliance, u osobních údajů navíc naráží na GDPR |
| hodnota odvozená z nevalidního zdroje | přenáší chybu dál a maskuje ji |
| 100% prázdný sloupec | není z čeho odvozovat; rozhodni mezi „doplnit z registru" a „odstranit sloupec" |
| chybějící vazba (sirotek) | přiřazení k náhodnému rodiči je fabrikace; buď dohledat, nebo smazat |
| stav/typ ukončení, pro který neexistuje číselník | není proti čemu validovat; nejdřív číselník, pak plnění |
| plošná konstanta místo skutečné hodnoty | přesně tak vznikl původní defekt |

## Výstup imputátoru

- `_STD` sloupce s doplněnými hodnotami + příznak původu.
- **Tabulka před/po na každý atribut**: úplnost před, po, kolik doplněno kterou metodou,
  kolik zbývá a proč to nešlo.
- Seznam kolizí (uložená ≠ odvozená hodnota) → zpět do `dq-auditor` jako nález vnitřní
  nekonzistence.

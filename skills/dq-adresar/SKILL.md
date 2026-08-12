---
name: dq-adresar
description: Napojení adres na adresní registr (RÚIAN, UIR-ADR nebo jakýkoli národní address registry) — stavba match code, hierarchie indexů podle přesnosti (budova, ulice, obec), fuzzy párování bez diakritiky, měření match rate, řešení collation a rozsahových mezer registru (P.O. Box), přiřazení kódu adresního bodu a příznaku doručitelnosti. Krok remediace mezi dq-standardizator a dq-imputator. Použij vždy, když se pracuje s adresami a je potřeba je "napojit na registr", "spárovat s RÚIAN", "geokódovat", "doplnit kód adresy", "ověřit doručitelnost", "zjistit kolik adres je validních" — i když uživatel registr nepojmenuje. Keywords: RÚIAN, UIR-ADR, adresní registr, address registry, obohacování dat, data enrichment, match code, adresa_kod, geokódování, párování adres, doručitelnost, match rate.
---

# Adresář — napojení na adresní registr

Adresa bez kódu z registru je jen text. S kódem je to ověřená doručovací adresa, na kterou lze
navěsit geokódování, pricing podle lokality a identifikaci domácnosti. V pojišťovnictví je
geokódování přímo vstupem do sazby — povodňová zóna, riziko krádeže, vzdálenost od hasičů —
takže nenapojená adresa není jen nedoručitelná, ale i špatně naceněná.

Tenhle krok se v kurzu 4IZ562 jmenuje **obohacování dat (data enrichment)** — napojení na
externí autoritativní zdroj a převzetí hodnoty, kterou vlastní data nenesou. Adresní registr
je jen nejčastější případ; totéž platí pro registr firem nebo klasifikaci odvětví.

Pipeline: `dq-standardizator` → **dq-adresar** → `dq-imputator` → `dq-deduplikator`.
Předpoklad: adresa je už atomizovaná (`dq-parser`) a poštovní kód standardizovaný.

Příklady jsou z českého RÚIAN/UIR-ADR. Postup je stejný pro libovolný národní adresní registr
(USPS, Ordnance Survey, národní address registry) — mění se jen názvy sloupců a formát
poštovního kódu. Princip „match code + hierarchie přesnosti + zákaz fallbacku" platí všude.

## Nejdřív si ověř, jestli registr vůbec unese roli standardu

Než začneš párovat, zprofiluj **samotný registr**. Reálné nálezy z auditovaného UIR-ADR:

| Problém registru | Rozsah | Důsledek |
|---|---|---|
| registr zrušený a nahrazený | UIR-ADR mrtvý od 2012, nástupce RÚIAN | validace proti němu je principiálně nesmyslná |
| chybějící souřadnice | 2 305 082 z 2 597 960 řádků (88,7 %) bez X i Y | registr nepoužitelný pro geokódování |
| invertované znaménko souřadnic | 292 878 záznamů s X ≥ 0 nebo Y ≥ 0 | v S-JTSK konvenci nesmysl |
| chybějící PK a indexy | celá tabulka | JOIN nad 2,6 M řádky bez indexu = neúnosné |
| neúplná sesterská tabulka | 49 670 platných adres (1,91 %) chybí ve sloučené verzi | tiché ztráty při párování |
| porušená atomičnost v registru | 21 145 řádků má číslici v poli ulice | registr má tutéž vadu jako provoz |
| prázdný řetězec místo NULL | `cisor_pis` u 99,46 % řádků | `COALESCE` nezabere, nutný `NULLIF(TRIM(x),'')` |
| smíšená collation | latin1 vs utf8 | JOIN spadne nebo tiše nesparuje |
| **rozsahová mezera** | P.O. Boxy registr z principu neobsahuje | platné adresy budou vždy „nevalidní" |

Poslední řádek si zaslouž samostatný odstavec ve zprávě. Adresa s poštovní přihrádkou je
formálně v pořádku; to, že ji registr nezná, je limit zdroje, ne defekt dat. Bez toho vykážeš
100 % těchto řádků jako chybu.

## Match code — princip

Match code je hash normalizovaného adresního řetězce, spočítaný **stejným předpisem** na obou
stranách. Umožní JOIN přes jeden indexovaný sloupec místo porovnávání pěti textových polí.

```sql
ALTER TABLE PARTY_ADDRESS       ADD ADDR_CODE bigint, ADD ADDR_FMCODE char(32);
ALTER TABLE REF_UIRADR_MERGED   ADD MCODE_FUZZY char(32);
CREATE INDEX ix_addr_fmcode ON PARTY_ADDRESS (ADDR_FMCODE);
CREATE INDEX ix_ref_mcode   ON REF_UIRADR_MERGED (MCODE_FUZZY);
```

Normalizační předpis (musí být identický na obou stranách): odstranit diakritiku, lowercase,
odstranit vše kromě písmen a číslic, spojit složky pevným oddělovačem.

```sql
-- strana registru
UPDATE REF_UIRADR_MERGED SET MCODE_FUZZY = MD5(replaceDiacritics(LOWER(TRIM(CONCAT(
  COALESCE(psc,''),'_', COALESCE(TRIM(ulice),''), COALESCE(cisdom_hod,''),
  CASE WHEN NULLIF(TRIM(cisor_hod),'') IS NOT NULL THEN CONCAT('/',cisor_hod) ELSE '' END,
  '_', COALESCE(TRIM(obec),''),
  CASE WHEN mcast IS NOT NULL AND TRIM(obec) <> '' THEN CONCAT(' - ',TRIM(mcast))
       WHEN mcast IS NOT NULL                      THEN TRIM(mcast) ELSE '' END )))));

-- strana provozu
UPDATE PARTY_ADDRESS SET ADDR_FMCODE = MD5(replaceDiacritics(LOWER(TRIM(CONCAT(
  COALESCE(ADDR_ZIP_STD,''),'_', COALESCE(TRIM(ADDR_STREET_STD),''),'_',
  COALESCE(TRIM(ADDR_CITY_STD),'') )))));

UPDATE PARTY_ADDRESS a INNER JOIN REF_UIRADR_MERGED b ON b.MCODE_FUZZY = a.ADDR_FMCODE
SET a.ADDR_CODE = b.adresa_kod;
```

Když JOIN přes hash nic nevrátí, je to typicky **collation** obou hash sloupců, ne logika.
Sjednoť je explicitně a spusť znovu:

```sql
ALTER TABLE REF_UIRADR_MERGED MODIFY MCODE_FUZZY char(32)
  CHARACTER SET utf8 COLLATE utf8_general_ci NULL;
```

## Hierarchie indexů — a proč se nesmí fallbackovat

Adresní registr není plochý seznam adres, ale hierarchie územních prvků — adresní místo,
ulice, stavební objekt, katastrální území, obec, okres, kraj. Do jaké úrovně se trefíš, to
pak můžeš agregovat. Postav proto víc indexů podle přesnosti:

| Index | Klíč | Přesnost |
|---|---|---|
| `idx_bld` | PSČ + obec + ulice + číslo popisné | konkrétní budova |
| `idx_bldv` | PSČ + obec + číslo popisné (obce bez ulic) | konkrétní budova na vesnici |
| `idx_str` | PSČ + obec + ulice | jen ulice |
| `idx_zc` | PSČ + obec | jen obec |

```python
def match_code(zip5, city_n, street_n, num):
    if not zip5 or not city_n or not num: return None
    if street_n:
        return idx_bld.get((zip5, city_n, street_n, num))   # přesná budova, jinak None
    return idx_bldv.get((zip5, city_n, num))                # vesnice bez ulice
```

**Žádný fallback uvnitř jednoho sloupce.** Kód ulice nebo obce zapsaný do pole pro budovu
vypadá jako úspěšné napojení, ale ukazuje na *jinou budovu*. Tiše špatný kód je horší než
chybějící — propíše se do doručování, geokódování i do identifikace domácnosti.

To ale neznamená hrubší shodu zahodit. Znamená to nevydávat ji za přesnou: ulož každou úroveň
do vlastního sloupce a přidej příznak dosažené přesnosti.

| Sloupec | Obsah |
|---|---|
| `ADDR_CODE` | kód adresního místa (budova), jinak NULL |
| `ADDR_CODE_STR` / `ADDR_CODE_MUN` | kód ulice / obce, když budova nevyšla |
| `ADDR_MATCH_LEVEL` | `BLD` / `STR` / `MUN` / `NONE` |

Potřebnou přesnost si pak vybere konzument. Geokódování, pricing podle rizikové zóny,
doručování a household potřebují **budovu**. Agregovaný reporting (regulátorovi po PSČ nebo
okresech), regionální kampaň nebo plošný mailing si vystačí s ulicí či obcí — vynutit tam NULL
by report znehodnotilo.

Do zprávy patří obojí: „na úroveň budovy 63 %, na úroveň ulice 89 %."

## Fuzzy párování — co pomáhá a co ne

| Technika | Efekt |
|---|---|
| odstranění diakritiky na obou stranách | největší jednotlivý skok v match rate |
| lowercase + odstranění interpunkce a mezer | zásadní, prakticky zdarma |
| `COALESCE(obec, mcast, cobce)` na straně registru | pokryje řádky s prázdnou obcí (v UIR-ADR ~51 %) |
| první segment před pomlčkou z pole obce | řeší `Praha 5 - Smíchov` |
| oříznutí PSČ na 5 číslic + vyřazení placeholderů (`99999`, `00000`) | odstraní falešné shody |
| **fonetické algoritmy** (Soundex, Metaphone) | standardní nástroj unifikace, ale jazykově vázaný — obě klasické varianty jsou laděné na angličtinu a na češtině vyrábějí falešné shody. Použij variantu pro cílový jazyk, nebo je vynech a změř, o kolik shod tím přicházíš |

Zvyšování match rate dělej **iterativně a měř po každém kroku**. Skok o desítky procent po
jedné úpravě je podezřelý — zkontroluj vzorek, jestli nepáruješ nesmysly.

## Metriky do zprávy

```sql
SELECT ROUND(100.0*SUM(ADDR_CODE IS NOT NULL)/COUNT(*),2) match_rate_pct FROM PARTY_ADDRESS;
```

Vykazuj tři čísla zvlášť: match rate na úroveň budovy (jediná použitá), podíl adres
vyřazených kvůli chybějící složce (PSČ / obec / číslo) a podíl mimo rozsah registru (P.O. Box).
Součet musí dát 100 % — jinak něco mizí tiše.

Výchozí stav v auditované DB byl **0 % exact match** na trojici PSČ + obec + ulice, protože
čísla domů byla slitá v ulici a obec obsahovala celou hierarchii. Po parsingu a standardizaci
se párování rozjelo. To je nejnázornější before/after v celé remediaci — použij ho.

## Pivot na typ adresy

Pro golden record potřebuješ jeden kód na klienta a typ. Ber první nenull kód daného typu:

```python
def pivot_type(t):
    sub = addr[(addr["ADDR_TYPE"] == t) & addr["ADDR_CODE"].notna()]
    return sub.groupby("PARTY_ID")["ADDR_CODE"].first()

addr_piv = pd.DataFrame({"ADDR_PERM": pivot_type("R"),
                         "ADDR_CORR": pivot_type("C"),
                         "ADDR_OTHER": pivot_type("O")}).reset_index()
```

Pozor na typ adresy mimo číselník (v auditované DB kód `P` u 40 % adres). Rozhodni **před**
pivotem, jestli ho přemapuješ na existující kód, nebo doplníš do číselníku — jinak 40 % adres
z pivotu vypadne a golden record bude bez trvalé adresy.

## Návazné použití

- `dq-imputator` — z kódu adresy dohledá chybějící PSČ nebo obec.
- `dq-deduplikator` — kód adresy je složka klíče domácnosti.
- Příznak doručitelnosti = adresa má kód na úrovni budovy a není P.O. Box.

---
name: dq-deduplikator
description: Deduplikace a Master Data Management — stavba match code (MCODE) s hierarchií klíčů, klastrování duplicitních záznamů, výběr přeživšího záznamu (survivor), konsolidace golden recordu napříč klastrem, identifikace domácnosti (household), skóre kvality záznamu a porovnání variant klíče. Poslední krok remediace před dq-strazce. Použij, když se má "deduplikovat klienty", "najít duplicity", "golden record", "single version of truth", "sloučit záznamy", "identifikovat domácnost", "MDM". Keywords: deduplikace, MCODE, match code, golden record, survivor, household, MDM, klastr, single customer view.
---

# Deduplikátor — golden record

Poslední krok remediace. Cíl: **jedna entita = jeden záznam**, plus průkazný klíč, podle
kterého se dá klastr kdykoli zrekonstruovat.

Pipeline: `dq-imputator` → **dq-deduplikator** → `dq-strazce`.

**Bez předchozích kroků to nedělej.** Dedup nad nestandardizovanými daty najde zlomek duplicit
a naměřené číslo bude falešně nízké. `Nováková` a `NOVAKOVA` jsou pro exact match dvě osoby.

## Match code (MCODE) — hierarchie klíčů

MCODE je hash identity, ne řádku. Stavěj ho **kaskádou od nejsilnějšího klíče**; první, který
lze spočítat, vyhrává:

| Pořadí | Typ entity | Klíč | Podmínka |
|---|---|---|---|
| 1 | osoba | `MD5('RC' + identifikátor)` | identifikátor prošel **checksumem**, ne jen formátem |
| 2 | osoba | `MD5('FO' + datum_nar + pohlaví + jméno_match + příjmení_match)` | kompozit, když identifikátor chybí nebo je nečitelný |
| 3 | osoba | `MD5('SINGLE' + PK)` | nelze klastrovat → vlastní klastr velikosti 1 |
| 1 | firma | `MD5('ICO' + identifikátor)` | validní identifikátor subjektu |
| 2 | firma | `MD5('PO' + název_match)` | bez identifikátoru |
| 3 | firma | `MD5('SINGLE' + PK)` | fallback |

```python
def mcode(r):
    if r["PARTY_TYPE"] == "P":
        if r["RC_VALID"] and r["RC_STD"]:
            return md5("RC", r["RC_STD"])
        if r["DOB_KEY"] and r["LNAME_MATCH"] and r["FNAME_MATCH"]:
            return md5("FO", r["DOB_KEY"], r["GENDER_KEY"], r["FNAME_MATCH"], r["LNAME_MATCH"])
        return md5("SINGLE", r["PARTY_ID"])
    if r["CREGNUM_STD"]: return md5("ICO", r["CREGNUM_STD"])
    if r["NAME_MATCH"]:  return md5("PO",  r["NAME_MATCH"])
    return md5("SINGLE", r["PARTY_ID"])
```

Tři věci, na kterých to stojí:

- **Prefix v hashi** (`'RC'`, `'FO'`, `'ICO'`) brání kolizi mezi úrovněmi kaskády. Bez něj se
  můžou potkat dvě různé entity se shodným řetězcem.
- **Vlastní klastr místo NULL.** Záznam bez klíče musí dostat unikátní MCODE, jinak se všechny
  neidentifikovatelné záznamy slijí do jednoho falešného klastru.
- **Do klíče jdou `_MATCH` varianty**, ne display hodnoty. `Nováková` vs `NOVAKOVA` musí dát
  stejný klíč.

### `USABLE` vs `VALID` — nejdůležitější rozhodnutí

Rozliš dvě různé role identifikátoru:

| Role | Kritérium | Použití |
|---|---|---|
| **dedup signál** (`_KEY`, `USABLE`) | struktura je čitelná, i když checksum neprošel | interní kompozitní klíč — čitelný identifikátor pořád spolehlivě spojí dva záznamy téže osoby |
| **publikovaná hodnota** (`_STD`, `VALID`) | prošlo checksumem | export, odvozené atributy, reporting |

Když u nečitelného identifikátoru chybí i odvozené datum, použij jako **klíčový** fallback
původní uložené datum (je-li věrohodné) — maximalizuje dedup signál, aniž by se cokoli
nevalidního dostalo na výstup.

## Klastrování a survivor

V rámci klastru vyber jeden přeživší záznam. Skóre kombinuje kvalitu, aktivitu a čerstvost:

```python
df["_rec"]  = pd.to_datetime(df["POSLEDNI_AKTIVITA"], errors="coerce") \
                .map(lambda x: x.toordinal() if pd.notna(x) else 0)
df["_surv"] = df["DQM_VALID_SCORE"] * 3 + df["POCET_TRANSAKCI"] * 0.5 + df["_rec"] / 1e6
df = df.sort_values(["MCODE", "_surv", "PK"], ascending=[True, False, True])
df["SURV_RECORD_IND"] = (~df.duplicated("MCODE", keep="first")).astype(int)
```

Váhy zdůvodni ve zprávě — jsou to obchodní rozhodnutí, ne matematika. Deterministický
tiebreak (`PK` jako poslední kritérium) je povinný, jinak nejsou výsledky reprodukovatelné.

**Nemaž duplicity.** Označ je příznakem. Smazání zahodí evidenci a znemožní audit sloučení.

### Skóre kvality záznamu

Podíl splněných kontrol podle typu entity — vstup do survivor skóre i metrika do zprávy:

```python
def dqm(r):
    checks = []
    if r["PARTY_TYPE"] == "P":
        checks += [r["RC_VALID"], r["FNAME_STD"] is not None, r["LNAME_STD"] is not None,
                   r["DOB_STD"] is not None, r["GENDER_STD"] is not None]
    else:
        checks += [r["CREGNUM_STD"] is not None, r["NAME_STD"] is not None]
    checks.append(any(pd.notna(r.get(c)) for c in ("ADDR_PERM","ADDR_CORR","ADDR_OTHER")))
    checks.append(any(pd.notna(r.get(c)) for c in ("CONT_EMAIL","CONT_MOBILE")))
    return round(sum(1 for c in checks if c) / len(checks), 4)
```

Sada kontrol se **liší podle typu entity** — jinak firma nikdy nedosáhne plného skóre kvůli
polím, která pro ni nedávají smysl.

## Konsolidace golden recordu

Survivor nemusí být nejlepší v každém atributu. Po výběru propaguj v rámci klastru nejlepší
dostupnou variantu — tím se opraví i to, co registr v `dq-standardizator` nespravil:

```python
def coalesce_name(std_col, corrupt_col):
    clean = df[std_col].where(~df[corrupt_col].fillna(False) & df[std_col].notna())
    best = (pd.DataFrame({"MCODE": df["MCODE"], "v": clean}).dropna(subset=["v"])
              .groupby(["MCODE","v"]).size().reset_index(name="n")
              .sort_values(["MCODE","n"], ascending=[True,False])
              .drop_duplicates("MCODE").set_index("MCODE")["v"])
    repl = df["MCODE"].map(best)
    use  = (df[corrupt_col].fillna(False) | df[std_col].isna()) & repl.notna()
    return df[std_col].where(~use, repl)
```

Kritérium výběru: nepoškozená a neprázdná varianta, v rámci klastru nejčastější. `BE0ŠOVÁ`
se opraví na `BENEŠOVÁ`, pokud v klastru existuje čistý dvojník téže osoby.

Podobně u datumů: atribut typu „od kdy je klientem" ber jako **minimum přes celý klastr**, ne
per řádek — jinak sloučení dvou záznamů posune vznik vztahu dopředu.

```python
df["SINCE_STD"] = df.assign(_s=pd.to_datetime(df["SINCE_STD"], errors="coerce")) \
                    .groupby("MCODE")["_s"].transform("min").dt.date
```

## Household — identifikace domácnosti

Skupina osob na téže adrese se společným kořenem příjmení:

```python
def surname_root(match_lname):
    """De-gendered kořen: mužský i ženský tvar → stejný kořen."""
    r = match_lname.split()[0] if " " in match_lname else match_lname
    if   r.endswith("OVA") and len(r) > 4: r = r[:-3]   # přechýlené -ová
    elif r.endswith("A")   and len(r) > 3: r = r[:-1]   # adj. ženské -á i mužské -a
    elif r.endswith("Y")   and len(r) > 3: r = r[:-1]   # adj. mužské -ý
    return r or None

def hshld(r):
    if r["PARTY_TYPE"] != "P": return None              # jen fyzické osoby
    root = surname_root(r["LNAME_MATCH"])
    code = next((c for c in (r.get("ADDR_CORR"), r.get("ADDR_PERM"), r.get("ADDR_OTHER"))
                 if pd.notna(c)), None)
    if not root or code is None: return None
    return md5("H", root, int(code))
```

Tři pravidla:

- **Jen na úrovni budovy.** Kód ulice nebo obce udělá z celé vesnice jednu domácnost.
- **Bez adresy žádná domácnost.** NULL je správný výstup, ne fallback na příjmení.
- **Právnické osoby vynech.** Firmy na jedné adrese nejsou domácnost.

De-gendering je jazykově specifický. Pro jiný jazyk pravidla nahraď — bez nich `Novák` a
`Nováková` na téže adrese skončí ve dvou domácnostech.

## Porovnání variant klíče

Před finálním rozhodnutím spočítej **obě varianty** a rozdíl vykaž:

| Varianta | Klíč | Efekt |
|---|---|---|
| A (volnější) | jakýkoli čitelný identifikátor | víc sloučení, riziko falešně pozitivních |
| B (přísnější) | jen checksum-validní + kompozit | méně sloučení, bezpečnější |

```python
print(f"varianta A: {va} entit | varianta B (použito): {df['MCODE'].nunique()} entit")
```

Rozdíl mezi nimi je odhad nejistoty deduplikace. Napiš ho do zprávy i s tím, kterou variantu
jsi zvolil a proč.

## Výstup deduplikátoru

Denormalizovaná tabulka, jeden řádek na vstupní záznam, plus:

| Sloupec | Význam |
|---|---|
| `MCODE` | klíč klastru entity |
| `SURV_RECORD_IND` | 1 = přeživší (golden record), 0 = duplicita |
| `HSHLD_MCODE` | klíč domácnosti |
| `DQM_VALID_SCORE` | skóre kvality záznamu 0–1 |

Souhrn do zprávy: počet řádků, unikátních entit (MCODE), přeživších, duplicit
(řádky − přeživší), domácností, match rate na registr, podíl validních identifikátorů,
podíl vyplněných kontaktů.

---
name: dq-auditor
description: Zpráva auditora kvality dat — katalog zjištění se závažností a byznys dopadem, root-cause analýza, kvantifikace nákladů nekvality (COPQ) a ROI nápravy, matice užití dat, legislativní kontext (GDPR, AML, Solvency II), prioritizovaná nápravná opatření. Třetí krok DQ pipeline, po dq-validator. Použij, když se má "napsat zprávu auditora", "sepsat nálezy", "vyčíslit dopad nekvalitních dat", "spočítat COPQ nebo ROI", "prioritizovat opravy", "root-cause analýza". Keywords: zpráva auditora, audit report, findings, COPQ, ROI, root-cause, matice užití, byznys dopad, prioritizace.
---

# Auditor — zpráva, dopady, doporučení

Validator dodal čísla. Auditor z nich udělá **rozhodnutí**: co to firmu stojí, proč to vzniklo
a v jakém pořadí to opravit. Zpráva bez korunové částky a bez příčiny je jen výpis dotazů.

Pipeline: `dq-validator` → **dq-auditor** → remediace (`dq-parser`…). Standard v `dq-pipeline`.

## Struktura zprávy

```
Historie dokumentu (verze, datum, autoři, popis změn)
Manažerské shrnutí
  Účel a cíle · Pro koho je určeno · Omezení užití
  Byznys dopad (po rizicích, ne po tabulkách)
  Vyčíslení dopadů a doporučení (COPQ, náklady nápravy, ROI)
Terminologie
Metodika auditu (8 kroků + použité nástroje a přístup)
Rozsah auditu (univerzum, kritéria hodnocení, standardy)
Výsledky auditu (A/B/C/D/E po doménách + kvantifikace COPQ)
Závěry (vyjádření ke zjištěním, root-cause, doporučení, prioritizovaná opatření)
Mezery oproti předpokládaným kontrolám · Mimo rozsah · Omezení auditu
Přílohy (chybějící FK, návrhy nových číselníků)
```

**Manažerské shrnutí piš jako poslední, ale čte se první.** Musí obsahovat jednu částku,
jeden termín a jedno doporučení. Ne seznam tabulek.

Sekce **Omezení užití** a **Identifikovaná omezení při realizaci auditu** nejsou formalita:
audit platí ke snapshotu, k danému rozsahu a s danými zdroji. Bez toho je zpráva nepodložená.

## Metodika — 8 kroků

| # | Krok | Obsah |
|---|---|---|
| 1 | Plánování a rozsah | vymezení univerza: provozní tabulky + podpůrné číselníky a registry |
| 2 | Kontrolní rámec | referenční standardy: právní normy, státní registry, interní číselníky |
| 3 | Měřené charakteristiky | šest dimenzí (viz `dq-pipeline`) |
| 4 | Identifikace užití dat | matice užití: atribut → byznys proces |
| 5 | Metadatový repozitář | `information_schema` + sloupce pro naměřené dimenze |
| 6 | Profiling a validace | `dq-profiler` + `dq-validator` |
| 7 | Kvantifikace dopadů | COPQ přes procesy z matice užití |
| 8 | Formulace doporučení | root-cause + prioritizovaná opatření |

Krok 4 se nejčastěji vynechává — a bez něj nejde udělat krok 7. **Matice užití** mapuje každý
klíčový atribut na procesy, které na něm stojí (marketing, compliance/AML, regulatorní
reporting, pojistná matematika). Ta vazba je jediný most mezi „2 % chybí" a „stojí to X Kč".

## Katalog zjištění

Členěný po doménách (A hlavní entita, B kontakty, C adresy, D smlouvy a produkty,
E číselníky a referenční tabulky), v každé tabulka `ID | Atribut | Dimenze | Naměřeno |
Závažnost | Byznys dopad` a pod ní 3–5 odstavců interpretace, které nálezy spojují do vzorců.

Interpretační odstavce jsou to, co odlišuje zprávu od výpisu. Skládej je tematicky, ne po
řádcích: „Identifikace klienta a křížová konzistence", „Nefunkční komunikační kanály",
„Porušení atomičnosti a nemožnost validace", „Sirotci a historický import".

Ke každému velkému nálezu připoj **křížovou kontrolu** s příponou `b` (viz `dq-pipeline`).
Nález `A1` (konstanta „CZE") je podezření; `A1b` (2 633 klientů s literálem „cizinec" v rodném
čísle a u všech 100 % země „CZE") je důkaz.

Nezapomeň na kategorii, která nemá vlastní řádek: **průřezové architektonické defekty** —
single-table pattern bez podtypování, chybějící FK napříč celou DB, nekonzistentní naming
convention, záměna prefixů REF/LOV, tři různé implementace `DEL_FLAG`. Ty patří do samostatné
sekce, protože se nedají opravit v jedné tabulce.

## Root-cause analýza

Metoda „n-krát proč" dovedená k příčině, kterou lze **opravit procesně**, ne dotazem.
Typický výsledek — šest kořenových příčin, na které se namapují desítky nálezů:

| # | Kořenová příčina | Co z ní plyne |
|---|---|---|
| 1 | **Absence centrálního MDM hubu** | každý kanál zakládá klienty nezávisle bez porovnání s bází → duplicity, roztříštěná historie, chybné CLV |
| 2 | **Chybějící referenční integrita (FK)** | sirotci, hodnoty mimo číselníky, nelze zabránit budoucímu výskytu |
| 3 | **Zastaralé a neudržované referenční zdroje** | klasifikace i registry ~10 let neplatné → validace je principiálně nesmyslná |
| 4 | **Absence vstupních validací v aplikaci** | do pole rodného čísla lze vložit „cizinec", frekvence mimo číselník, smlouva na neaktivní produkt |
| 5 | **Nekonzistentní naming convention** | mix jazyků a case, záměna prefixů, `DESC` vs `DESCR` → generické ETL musí ošetřovat varianty |
| 6 | **Absence Data Governance** | není vlastnictví dat, nejsou stewardi, není datový katalog → nikdo neodpovídá za kvalitu atributu |

Pravidlo: ke každé příčině uveď **evidenci z dat** (konkrétní nález s číslem) a **odkaz na
architekturu** (který systém, který kanál, kdy vzniká). Diagram toku dat mezi kanály a
systémy vysvětlí příčinu líp než odstavec.

Kaskády pojmenuj: duplicitní adresy nejsou samostatný problém, ale důsledek chybějící
deduplikace klientů (příčina #1). Bez toho se opravuje symptom.

## COPQ — kvantifikace nákladů nekvality

Postup na každý proces z matice užití:

1. **Zasažené univerzum** — kolik entit je defektem vyřazeno z procesu.
2. **Jednotková hodnota** — kolik proces vydělá na jedné entitě (marže, provize, hodnota smlouvy).
3. **Konverze / pravděpodobnost** — kolik z nich by reálně dopadlo.
4. **Frekvence** — ročně, nebo jednorázový potenciál (**a v tabulce to rozliš**).
5. **Ztráta = 1 × 2 × 3 × 4.**

Typické procesy: cross-sell kampaně (ročně), up-sell (jednorázový potenciál), retence (ročně),
pojistně-matematické výpočty a technické rezervy (jednorázová chybná alokace).

Proti tomu **náklady nápravy**: člověkodny × sazba × velikost týmu, plus licence/napojení
na registry. ROI počítej **konzervativně** — jen z nejjistějšího proudu, ne ze součtu všech.

Reálný příklad struktury výsledku:

| Položka | Hodnota |
|---|---|
| COPQ celkem (konzervativně) | > 260 mil. Kč |
| z toho cross-sell (ročně) | 7 662 600 Kč |
| z toho up-sell (jednorázový potenciál) | 153 252 000 Kč |
| z toho retence (ročně) | 38 313 000 Kč |
| z toho chybná alokace technických rezerv | ~61 600 000 Kč |
| Náklady projektu nápravy | 2 400 000 Kč (80 dní × 5 lidí) |
| **ROI (jen z cross-sell)** | **219 %** — návratnost v prvním roce |

Každý předpoklad vypiš explicitně (počet klientů, marže, konverze). Auditovaný musí být
schopen ti čísla oponovat — o to jde. Nepodložený odhad vydávaný za výpočet je horší než
přiznaný odhad.

## Legislativní kontext

Ke každé doméně připoj normu, kterou defekt porušuje. Nejde o ozdobu — mění to závažnost
z „střední" na „kritické" a je to nejsilnější argument pro rozpočet:

| Norma | Co vyžaduje | Typický defekt, který ji porušuje |
|---|---|---|
| **GDPR** (2016/679, čl. 5) | přesnost údajů, minimalizace, právo na výmaz | osobní údaje bez identifikovatelného subjektu (sirotci); duplicity znemožňující výmaz |
| **AML/KYC** (253/2008 Sb.) | identifikace klienta, PEP screening, sankční seznamy | plošná konstanta v zemi původu a místě narození; text v rodném čísle |
| **Solvency II** (2009/138/ES) | správné technické rezervy a SCR | chybný věk, zrušené smlouvy tvářící se jako aktivní, antedatace |
| **Zákon o pojišťovnictví** (277/2009 Sb.) | doručování dokumentů, evidence smluv | nedoručitelné adresy, chybějící PSČ |
| **Zákon o el. komunikacích** (480/2004 Sb.) | pravidla obchodních sdělení | rozesílka na systematicky poškozené e-maily → blacklisting domény |
| **Zákon o základních registrech** (111/2009 Sb.) | RÚIAN jako autoritativní zdroj adres | validace proti registru zrušenému v roce 2012 |

## Doporučení a prioritizace

Dvě vrstvy, obě povinné:

**Strategická** — MDM hub, zavedení FK, napojení na aktuální státní registry, sjednocení
collation, vstupní kontroly, atomizace adres, naming standard, Data Governance, kontinuální
monitoring, rozšíření datového modelu.

**Specifická nápravná opatření** — tabulka `Priorita | Tabulka/Oblast | Opatření | Přínos`,
kde každý řádek je konkrétní a spočítaný: „Opravit 22 694 e-mailů (`#` → `@`, `&` → `.`)
a zavést regex validaci na vstupu → funkční elektronická komunikace".

Priorita se řídí kombinací: závažnost × počet zasažených × náklad opravy. Vysoká = regulace
nebo peníze; střední = analytika a údržba; nízká = hygiena.

**Ke každému opatření patří prevence.** Oprava dat bez opravy vstupní kontroly znamená, že se
defekt vrátí — a to napiš explicitně: „retrospektivně prověřit 23 477 smluv **a** zavést
aplikační kontrolu `CNTR_VALIDFROM ∈ [PRODUCT.VALID_FROM, PRODUCT.VALID_TO]`". Prevenci
detailně řeší `dq-strazce`.

## Sekce, které se vyplatí nevynechat

- **Mezery oproti předpokládaným kontrolám** — co se očekávalo, že existuje, a neexistuje.
- **Předměty mimo rozsah této verze** — poctivé vymezení, co se neauditovalo.
- **Přílohy** — kompletní seznam chybějících FK; návrh číselníku pro atribut, který žádný nemá.

Ten poslední bod je vlastní žánr nálezu: atribut vyplňovaný bez existující autoritativní sady
hodnot. Nejde jen o neúplnost — i kdyby se doplnil, není proti čemu ho validovat. Návrh
číselníku patří do přílohy.

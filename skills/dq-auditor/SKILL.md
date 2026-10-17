---
name: dq-auditor
description: The data quality auditor's report. A finding catalogue with severity and business impact, root cause analysis, quantification of the cost of poor quality (COPQ) and the ROI of remediation, the data usage matrix, the legal context (GDPR, AML, Solvency II), and prioritised corrective actions. Third step of the DQ pipeline, after dq-validator. Use it when someone wants to "write the audit report", "write up the findings", "quantify the impact of bad data", "compute COPQ or ROI", "prioritise fixes", "root cause analysis". Keywords: audit report, findings, COPQ, ROI, root cause, usage matrix, business impact, prioritisation.
---

# Auditor: report, impact, recommendations

The validator delivered numbers. The auditor turns them into **decisions**: what it costs the company, why
it happened and in what order to fix it. A report without a money figure and without a cause is just a
dump of queries.

Pipeline: `dq-validator` → **dq-auditor** → remediation (`dq-parser`…). Standard in `dq-pipeline`.

## Report structure

```
Document history (version, date, authors, description of changes)
Executive summary
  Purpose and goals · Intended audience · Limitations of use
  Business impact (by risk, not by table)
  Quantified impact and recommendations (COPQ, cost of remediation, ROI)
Terminology
Audit methodology (12 steps + tools and approach used)
Audit scope (universe, evaluation criteria, standards)
Audit results (A/B/C/D/E by domain + COPQ quantification)
Conclusions (opinion on the findings, root cause, recommendations, prioritised actions)
Gaps against expected controls · Out of scope · Audit limitations
Appendices (missing FKs, proposals for new code lists)
```

**Write the executive summary last, but it is read first.** It must contain one money figure, one deadline
and one recommendation. Not a list of tables. It fits on **one page**; you are writing for the board, not
for the DBA.

**Discuss the conclusions in advance** with the managers they concern, before the board sees them. It is an
ISACA standard and has three reasons: an anomaly may have a legitimate business explanation you cannot see
in the data; nobody likes surprises at the board; and a recommendation gets implemented by the person who
heard it first from you, not from the screen.

Vocabulary for an academic or audit audience: finding → **defect** or **discrepancy**, root cause →
**primary cause**, corrective actions → **recommended actions**. The usage matrix keeps its name (or bus
matrix).

The **Limitations of use** and **Limitations identified during the audit** sections are not a formality:
the audit holds for a snapshot, for a given scope and with given sources. Without them the report is
unsupported.

## Methodology: 12 steps

| # | Step | How it is done |
|---|---|---|
| 1 | Audit plan | scope, schedule, resources, deliverables |
| 2 | Defining the audit universe | inventory and table roles (`dq-profiler`) |
| 3 | Defining the audit frame | standards, best practice, reference sources, legal norms, validation rules |
| 4 | Defining the measured characteristics | data characteristics (`dq-pipeline`) |
| 5 | Identifying data use and prioritising | usage matrix: attribute → business process |
| 6 | Defining the impact of poor data | an impact model per process, before any measurement |
| 7 | Basic technical profiling | `dq-profiler` |
| 8 | Measuring characteristics with validation rules | `dq-validator` + writing to the MDR |
| 9 | Identifying recommended external reference sources | registries and code lists missing from the audit (`dq-address`) |
| 10 | Quantifying impact | COPQ through the processes in the usage matrix |
| 11 | Summarising the audit conclusions | finding catalogue, root cause, cross-cutting defects |
| 12 | Formulating recommendations | strategic layer + prioritised actions |

There is one inconspicuous but essential point in that order: **steps 5 and 6 come before profiling**. The
impact model is defined before the first number lands; otherwise you unconsciously fit the impact to what
the data showed, and the report becomes a defence of findings instead of a measurement of risk.

Step 9 is also an output, not preparation: the list of registries nobody can connect to belongs in the
recommendations. A missing reference source is a finding.

The twelve steps combine established methodologies; when you defend the report, name the source: **TIQM**
(English) for the cost of poor quality, **Ten Steps** (McGilvray) for the usage matrix and the POSMAD life
cycle, **TDQM** (Wang, MIT) for the IP-Map of data flow, **ISACA ITAF / COBIT** (standards S7, G20) for the
requirements and independence of the audit report. Without naming the framework it reads as one person's ad
hoc practice, not as an auditable methodology.

Step 5 is the one most often skipped, and without it step 10 cannot be done. The **usage matrix** maps every
key attribute to the processes that rely on it (marketing, compliance/AML, regulatory reporting, actuarial
work). That link is the only bridge between "2 % are missing" and "it costs X".

Steps 4 and 5 also include a **subjective assessment of quality**: an interview or questionnaire with data
users and stewards. A measured number says how many values are missing; only the user says whether that is
a defect or a legitimate state of the process. Findings that do not show in the data (working around the
system, shadow spreadsheets, distrust of a report) only come out here.

## Finding catalogue

Split by domain (A main entity, B contacts, C addresses, D contracts and products, E code lists and
reference tables), each with a table `ID | Attribute | Dimension | Measured | Severity | Business impact`
and below it 3–5 paragraphs of interpretation that connect the findings into patterns.

The interpretation paragraphs are what separates a report from a dump. Compose them by theme, not row by
row: "Client identification and cross-consistency", "Dead communication channels", "Broken atomicity and
impossible validation", "Orphans and a historical import".

For every major finding add a **cross-check** with the suffix `b` (see `dq-pipeline`). Cross-testing is a
standard method; the `b` suffix and the three to five paragraphs per domain are my convention, not
established notation. Finding `A1` (constant "CZE") is a suspicion; `A1b` (2,633 clients with the literal
"cizinec" in the birth number and country "CZE" in 100 % of them) is evidence.

Do not forget the category without a row of its own: **cross-cutting architectural defects**: the
single-table pattern without subtyping, missing FKs across the whole database, an inconsistent naming
convention, swapped REF/LOV prefixes, three different implementations of `DEL_FLAG`. They belong in a
separate section, because they cannot be fixed in one table.

## Root cause analysis

The **"5 Whys"** method carried through to a cause that can be **fixed in the process**, not by a query.
When there can be several causes at once, sort them with an **Ishikawa (fishbone) diagram** into processes,
people and technology; otherwise you stop at one and miss the rest. A typical result: six root causes that
dozens of findings map onto:

| # | Root cause | What follows from it |
|---|---|---|
| 1 | **No central MDM hub** | every channel creates clients independently without comparing against the base → duplicates, fragmented history, wrong CLV |
| 2 | **Missing referential integrity (FK)** | orphans, values outside code lists, no way to prevent future occurrences |
| 3 | **Obsolete and unmaintained reference sources** | classifications and registries ~10 years out of date → validation is meaningless in principle |
| 4 | **No input validation in the application** | the birth number field accepts "cizinec", frequency outside the code list, a contract on an inactive product |
| 5 | **Inconsistent naming convention** | mixed languages and case, swapped prefixes, `DESC` vs `DESCR` → generic ETL has to handle variants |
| 6 | **No data governance** | no data ownership, no stewards, no data catalogue → nobody is accountable for an attribute's quality |

Rule: for every cause give **evidence from the data** (a specific finding with a number) and **a reference to
the architecture** (which system, which channel, when it arises). Formally: you locate the cause through
**data lineage** (what the value went through) and **data provenance** (where it came from). A diagram of the
data flow between channels and systems explains the cause better than a paragraph. The formal notation for it
is the **IP-Map** (TDQM, MIT): data sources, transformations, user roles and DQ activities marked separately;
the picture then shows which stretch of the flow has no control at all.

Name the cascades: duplicate addresses are not a separate problem but a consequence of missing client
deduplication (cause #1). Without that you fix a symptom.

## COPQ: quantifying the cost of poor quality

The procedure for each process in the usage matrix:

1. **Affected universe**: how many entities the defect removes from the process.
2. **Unit value**: how much the process earns on one entity (margin, commission, contract value).
3. **Conversion / probability**: how many of them would actually convert.
4. **Frequency**: yearly, or a one-off potential (**and keep them apart in the table**).
5. **Loss = 1 × 2 × 3 × 4.**

Typical processes: cross-sell campaigns (yearly), up-sell (one-off potential), retention (yearly), actuarial
calculations and technical provisions (one-off misallocation).

Two are overlooked and yet the most expensive:

- **CLV degradation**: a client split into two records falls lower in segmentation, gets worse care and is more
  likely to leave. The loss is not one campaign but the rest of their life cycle.
- **Devalued propensity models**: without a working phone and e-mail there is no follow-up and the response
  rate drops by an order (in the model example from 20 % to 10 %). The model is fine; you just cannot act on
  it.

Against that, the **cost of remediation**: person-days × rate × team size, plus licences/registry
connections. Compute ROI **conservatively**, only from the most certain stream, not from the sum of all.
That is stricter than the textbook model, which computes ROI from the sum of all quantified losses. The
reason: a number that stands on one well-supported stream is harder to knock down at the table with the
auditee than a sum of four estimates.

A real example of the shape of the result:

| Item | Value |
|---|---|
| Total COPQ (conservative) | > CZK 260 M |
| of which cross-sell (yearly) | CZK 7,662,600 |
| of which up-sell (one-off potential) | CZK 153,252,000 |
| of which retention (yearly) | CZK 38,313,000 |
| of which misallocation of technical provisions | ~CZK 61,600,000 |
| Cost of the remediation project | CZK 2,400,000 (80 days × 5 people) |
| **ROI (cross-sell only)** | **219 %**, payback in the first year |

List every assumption explicitly (number of clients, margin, conversion). The auditee must be able to
challenge your numbers; that is the point. An unsupported estimate passed off as a calculation is worse than
an admitted estimate.

**How to present it.** A table of numbers will not get through the board on its own. Use a **waterfall
chart**: on the left the starting cost of poor quality, the individual remediation projects as bars going
down, on the right the target state. It reconciles "it costs us X" with "after remediation Y" and explains for
you where the money goes. Add one **catastrophe story** from operations to the hard numbers: a concrete
anecdote about what the defect caused sells the budget better than the fourth decimal place.

## Legal context

For every domain add the norm the defect breaches. It is not decoration: it changes severity from "medium"
to "critical" and is the strongest argument for budget. The examples are from the EU and the Czech
Republic; use the equivalent norms of your jurisdiction:

| Norm | What it requires | Typical defect that breaches it |
|---|---|---|
| **GDPR** (2016/679, Art. 5) | accuracy, minimisation, right to erasure | personal data without an identifiable subject (orphans); duplicates that make erasure impossible |
| **AML/KYC** (Czech Act 253/2008) | client identification, PEP screening, sanctions lists | a blanket constant in country of origin and place of birth; text in the birth number |
| **Solvency II** (2009/138/EC) | correct technical provisions and SCR | wrong age, cancelled contracts appearing active, back-dating |
| **Insurance Act** (Czech Act 277/2009) | delivery of documents, contract records | undeliverable addresses, missing postcodes |
| **Electronic Communications Act** (Czech Act 480/2004) | rules for commercial communications | mailing to systematically corrupted e-mails → domain blacklisting |
| **Basic Registers Act** (Czech Act 111/2009) | RÚIAN as the authoritative address source | validation against a registry abolished in 2012 |

## Recommendations and prioritisation

Two layers, both mandatory:

**Strategic**: an MDM hub, introducing FKs, connecting to current state registries, unifying collation, input
controls, atomising addresses, a naming standard, data governance, continuous monitoring, extending the data
model.

**Specific corrective actions**: a table `Priority | Table/Area | Action | Benefit`, where every row is
concrete and quantified: "Fix 22,694 e-mails (`#` → `@`, `&` → `.`) and introduce regex validation at input →
working electronic communication".

Priority follows the combination severity × number affected × cost of fix. High = regulation or money; medium
= analytics and maintenance; low = hygiene.

That formula is a coarse filter, not a verdict. Above it decides the **benefit × cost matrix**: **quick wins**
(high impact, cheap fix) go to the top, architectural projects after them. And confront the result with the
subjective assessment: an attribute with a high error rate that nobody uses in practice has low priority
regardless of what the formula computed.

The strategic recommendations include **assigning roles** (data owner, data steward, data product manager) to
processes and data products, ideally with a RACI matrix. A recommendation without a name is a wish.

**Every action comes with prevention.** Fixing the data without fixing the input control means the defect
returns, and say so explicitly: "retrospectively review 23,477 contracts **and** introduce the application
control `CNTR_VALIDFROM ∈ [PRODUCT.VALID_FROM, PRODUCT.VALID_TO]`". Prevention in detail is handled by
`dq-guard`.

## Sections worth keeping

- **Gaps against expected controls**: what was expected to exist and does not.
- **Items out of scope of this version**: an honest statement of what was not audited.
- **Appendices**: the complete list of missing FKs; a proposed code list for an attribute that has none.

That last point is a kind of finding of its own: an attribute filled in without any existing authoritative
set of values. It is not only incompleteness; even if it were filled, there is nothing to validate it against.
The code list proposal belongs in the appendix.

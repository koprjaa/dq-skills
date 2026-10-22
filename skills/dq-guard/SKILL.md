---
name: dq-guard
description: Preventing defects and monitoring data quality. Foreign keys and CHECK constraints, fixing data types, a DQ firewall at input, continuous measurement of metrics with alerting, the metadata repository, data contracts with SLAs and SLIs, and data governance (data ownership, stewards, data catalogue, governance bodies, maturity model). The last step of the DQ pipeline, after dq-deduplicator. Use it when someone wants to "stop errors from coming back", "add input controls", "set up data quality monitoring", "add FKs and constraints", "data governance". Keywords: prevention, DQ firewall, constraint, foreign key, monitoring, alerting, data governance, data steward, data catalogue, observability, data contract, SLA, SLI, maturity model.
---

# Guard: prevention and monitoring

Cleaned data without a fixed cause gets dirty again. This step is the only one with a lasting effect; everything
before it is a one-off clean-up.

Pipeline: `dq-deduplicator` → **dq-guard**. Input: the list of missing FKs from `dq-profiler` and the root causes from
`dq-auditor`.

## Four layers of defence

| Layer | Where | Catches | Cost |
|---|---|---|---|
| 1. data type | DDL | structural nonsense (padding, truncation, lost zeros) | a one-off migration |
| 2. constraint | DB | invalid values, orphans, duplicates | low, but rejects existing bad data |
| 3. DQ firewall | application / ETL | everything else, including cross-field rules | medium, needs maintenance |
| 4. monitoring | outside the transaction | what flowed through, and trends | low, but reacts after the fact |

Deploy from the bottom up. A constraint without fixed data will not pass; that is why this step comes **after
remediation**, not before it.

Two things that are easy to overdo with layers of defence:

- **A control does not create quality, it only measures it.** The firewall and observability are reactive: they say
  there is a problem. When they become the main tool, you just document a broken process more elegantly. Quality is
  produced only by fixing the process where the defect arises, and by the people who run it.
- **Controls cost money.** Developing and running the firewall, services and monitoring are prevention and appraisal
  costs; against them stand the costs of poor quality. The goal is not 100 % quality at the price of a paralysed
  business, but **the optimum where the sum of both is lowest**. For every proposed control, say what it costs and
  what it saves.

## 1. Data types

| Defect | Fix | Why |
|---|---|---|
| `char(n)` on variable-length text | `varchar(n)` | the source of padding and silent truncation; but for a code with a guaranteed fixed length (company ID `char(8)`) `char` is right |
| a fixed length on a code that may grow | a longer `varchar` | a `char(5)` postcode architecturally excludes foreign addresses |
| a numeric type on a code with a leading zero | `varchar` | lost zeros in identifiers and postcodes |
| a text Y/N flag | `BOOLEAN NOT NULL DEFAULT` | three different implementations of the same thing (`char(1)` / `char(4)` / `tinyint`) |
| a one-character PK on a code list | a surrogate `auto_increment` + `UNIQUE` on the code | fragile, collisions when extended |
| nullable on a logically mandatory column | `NOT NULL` | "mandatory" must be enforced by the schema, not by documentation |
| mixed collation | uniform across the whole DB | silent non-matches and errors in JOINs |

Unify collation **globally, not table by table**; otherwise a new pair arises that cannot JOIN.

## 2. Constraints

```sql
-- referential integrity (first check that the data passes)
SELECT COUNT(*) FROM child c LEFT JOIN parent p ON c.PARENT_ID = p.ID WHERE p.ID IS NULL;
ALTER TABLE child ADD CONSTRAINT fk_child_parent
  FOREIGN KEY (PARENT_ID) REFERENCES parent(ID);

-- value domain through a code list
ALTER TABLE PARTY_ADDRESS ADD CONSTRAINT fk_addr_type
  FOREIGN KEY (ADDR_TYPE) REFERENCES LOV_ADDR_TYPE(CODE);

-- business uniqueness (a surrogate PK does not guarantee it)
ALTER TABLE PARTY_CONTACT ADD CONSTRAINT uq_party_cont UNIQUE (PARTY_ID, CONT_TYPE);

-- logical rules
ALTER TABLE PROD_CONTRACT ADD CONSTRAINT ck_valid_range
  CHECK (CNTR_VALIDTO IS NULL OR CNTR_VALIDFROM <= CNTR_VALIDTO);
ALTER TABLE PART_PARTY ADD CONSTRAINT ck_rc_format
  CHECK (PARTY_RC IS NULL OR PARTY_RC REGEXP '^[0-9]{9,10}$');
```

Add FKs **as the complete list from the profiler**, not at random. A database where not one of 9 code list and 8
reference relationships has an FK is not fixed by three constraints.

Before adding an FK, decide what to do with existing violations: delete, reassign, or extend the code list. That
decision belongs in the report, because it is irreversible.

A constraint that cannot be introduced is also a result: "an FK on `CNTR_CANCTYPE` cannot be introduced because no
code list exists for the attribute" → first a code list proposal, then the constraint.

## 3. DQ firewall

Validation at input, before a value is stored. You already know the rules from the audit; every finding is one rule.

| Category | Rule | Finding that triggered it |
|---|---|---|
| format | identifier checksum (mod 11), regex for e-mail, postcode, phone | the text `cizinec` in the birth number field |
| domain | the value must exist in the code list | 91,693 transactions with a code outside the code list |
| cross-field | a derived attribute must match its source | 24,642 persons with an identifier ↔ date of birth mismatch |
| temporal | transaction date within the product's validity | 23,477 contracts outside the product's validity |
| range | age 0–130, date of birth not in the future | a stored age off by up to 83 years |
| mandatory | by entity type, not globally | otherwise a company must fill in a date of birth |
| placeholders banned | a blacklist of `NA`, `9999999999`, `99999` | placeholders as a substitute for NULL |
| deduplication at input | compare a new record with the existing base (MDM hub) | duplicates created in parallel from several channels |

The last row is the most expensive and the most effective. Without a central comparison when an entity is created,
duplicates are produced faster than you can clean them up.

The firewall must be able to **reject and explain**. An error message "invalid value" drives operators to work around
it (hence placeholders and shadow taxonomies). A message with a reason and an offer of the correct value does not.

## 4. Monitoring

Run the metrics from `dq-validator` periodically with the same recipe and store **a time series**, not just the
current value.

```sql
CREATE TABLE DQ_HISTORY (
  MEASURED_AT datetime NOT NULL, TABLE_NAME varchar(64), COLUMN_NAME varchar(64),
  DIMENSION varchar(20), SCORE decimal(6,4), ROWS_TOTAL bigint, ROWS_FAILED bigint,
  PRIMARY KEY (MEASURED_AT, TABLE_NAME, COLUMN_NAME, DIMENSION));
```

Alert on **change**, not on an absolute value. 92 % completeness may be normal; a drop from 99 % to 92 % in a day is an
incident. Thresholds: a sudden drop of a score, a new value outside the code list, a rise in the share of placeholders,
a fall of the match rate to the registry, a volume outside the expected range.

For "change" there are measures you can defend better than an eyeballed threshold: **PSI** (population stability
index) and **Kullback-Leibler divergence** for a shift of the value distribution, the **Kolmogorov-Smirnov test** for
agreement with a reference period. For stability over time a **Shewhart control chart** fits: the mean, bands at ±1/2/3
sigma, LCL and UCL; it tells ordinary noise from a signal better than a fixed limit.

A shift of the value distribution is called **data drift**, a change of structure **schema drift**. It follows that
profiling is not a one-off preparation before an audit: it should run **on every load**, not once per project.

The difference from data observability: monitoring measures **the quality of content** against defined rules,
observability watches **the behaviour of the pipeline**, through five pillars: freshness, value distribution, volume,
schema and lineage. You need both; schema drift silently breaks the rules themselves.

Keep the metadata repository (`DQM_MDR`) current; it is at the same time the monitoring configuration and the
documentation. When it does not cover the whole universe, monitoring has blind spots.

## Data contracts

Monitoring and observability detect schema drift **only in production**. The proactive counterpart is the **data
contract**: an agreement between the producer of a data product and its consumer about what is guaranteed.

| Part of the contract | Content |
|---|---|
| schema | columns, types, mandatory or not, allowed values |
| semantics | what the attribute means and what does not belong in it |
| **SLA** | the promised level: freshness, availability, delivery frequency |
| **SLI** | the specific measured number that proves the SLA is met |
| version and change policy | what a breaking change is and how long the migration window is |

The contract lives in the catalogue and is **enforced in CI**: a schema change that breaks it fails at build, not in
the nightly batch. The difference from monitoring is the moment: the contract rejects the problem before deployment,
monitoring describes it afterwards. You want both.

State the SLA measurably. Not "the data will be current", but "snapshot by 06:00, completeness of mandatory attributes
≥ 99 %, measured daily". Monitoring then does not watch metrics in general but **contract fulfilment**.

## 5. Data governance

Technical measures last as long as someone is accountable for them. Without that, after a year everything returns to
its original state:

| Element | Content |
|---|---|
| data ownership | a specific unit is accountable for each domain, not "IT" |
| data steward | a named role for the quality of specific attributes, with the authority to change rules |
| data catalogue | what an attribute means, where it comes from, who changes it, which rules apply |
| lineage and provenance | where a value came from and what it went through; without it the cause cannot be traced; column-level lineage is obtained by parsing the SQL itself (SQLGlot), with grammar tuning for non-standard dialects |
| code list management | a process for introducing a new value, otherwise a shadow taxonomy emerges |
| regular re-audit | metrics are reported and acted on, not just measured |

`DQM_MDR` is a minimal local variant of a catalogue, enough to audit one schema. The target state of governance is a
**federated data catalogue** (DataHub, Atlan, Collibra) that connects technical, operational and business metadata
(lineage, data contracts, business glossary) across sources. Do not pass a MySQL table off as a data catalogue; it is
its seed.

Roles alone are not enough; they need a structure and a mandate:

| Body | Who and what |
|---|---|
| **Executive sponsor** | a senior manager who provides vision and budget |
| **CDO** | the business counterweight to the CIO, accountable for data as an asset |
| **Steering committee** | oversight and escalation |
| **Data governance council** | domain owners; they approve standards, policies and target levels |
| **Data governance office** | the executive team: stewards, custodians, analysts |

**A steward is not one role.** The *business steward* (SME) defines the meaning of an attribute, the code lists and
the priorities of rules; the *technical steward* is responsible for types, indexes, ETL and storage. When it is one
person, one of the two roles is in practice not done.

**Governance need not be equally strict everywhere.** Key data elements under regulation (Solvency II, GDPR, AML) get a
hard regime with approvals; ad hoc analysis and data science a looser one, otherwise you only slow down people who are
not risking anything. A two-speed regime is intentional, not inconsistency, but it must be written down what falls
where.

Measure where it should get to with a **maturity model** (Initial → Optimized) across the dimensions people, processes,
standards and technology. Without it, governance is an endless project with no definition of done.

Prevention is not only a control on the input form. The **POSMAD** information life cycle (plan, obtain, store, share,
maintain, apply, dispose) shows everywhere a defect arises; sharing, maintenance and disposal are as valid as
acquisition. Walk the cycle step by step and ask at each step which control is missing.

A missing category in a code list (example: a self-employed person who has both a personal and a business identifier)
is a governance problem, not a data problem. As long as nobody may extend the code list, operators will fill in the
nearest wrong value.

## Output of the guard

- A DDL script: types, FKs, UNIQUE, CHECK, in the order in which it will pass.
- A catalogue of firewall rules, each with a reference to the finding that triggered it.
- Definitions of monitored metrics, thresholds and alert recipients.
- A governance proposal: roles, ownership, the code list change process.
- **A list of measures that could not be deployed and what blocks them**: input for the next round.

## Common mistakes

| Mistake | Consequence | Instead |
|---|---|---|
| Adding constraints before remediation | they fail on existing data | deploy bottom up, after the clean-up |
| A few FKs picked at random | integrity stays broken | the complete missing-FK list from `dq-profiler` |
| Adding an FK without deciding about violations | an irreversible deletion or reassignment nobody chose | decide (delete, reassign, extend the code list) and record it in the report |
| A firewall that only says "invalid value" | operators work around it with placeholders | reject with a reason and offer the correct value |
| Alerting on absolute values | normal levels page people, real drops do not | alert on change (PSI, control chart) |
| Controls with no owner | everything returns to the old state within a year | data owner and steward per domain |

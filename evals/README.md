# Evals

Scenarios that test whether the skills change what an agent does, not only whether the files are well
formed (that is `tests/skills.rs`).

Each scenario has a task, invented context data and grading criteria. Run the task twice in a fresh
session: once without the skills (baseline) and once with them, then grade both against the criteria.
A criterion is only useful when the baseline tends to fail it.

```
evals/scenarios/<name>/
  scenario.md   task, setup, grading criteria
  context/      invented data the agent works on
```

Rules:

- Only invented or course data. Never client data, never a database you do not own.
- Build databases outside the repository (`/tmp`), so a run cannot change the fixture.
- Record each run in the Results table of the scenario (date, model, passed, missed). There is no runner yet.

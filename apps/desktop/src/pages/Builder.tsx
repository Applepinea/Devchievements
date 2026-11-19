/**
 * The Achievement Builder (spec 22).
 *
 * # What this page is and is not
 *
 * Spec 22 says the first release ships the JSON advanced mode and puts the visual
 * editor in a later version. This page is the middle ground the checklist asks for: a
 * form that builds the rule, shows the JSON it will send, and lets the user edit that
 * JSON directly. Both paths go through `add_achievement`, so the limits (depth 5,
 * 50 conditions, step budget) are enforced once, in Rust, and cannot be bypassed by
 * whichever editor the user prefers.
 *
 * # Why the form is not a full query builder
 *
 * The rule model has three shapes (condition / all / any) and four metrics. Building
 * arbitrary nesting in a form is a large UI for a feature whose spec-defined audience
 * is "people who can write the JSON". The form therefore covers the flat rule and one
 * level of AND/OR, and the JSON box covers everything else. Every metric the rule
 * model actually has is offered; nothing is invented.
 */

import { useCallback, useEffect, useMemo, useState } from "react";

import {
  addAchievement,
  listAchievements,
  listBuiltinAchievements,
  removeAchievement,
  type BuiltinAchievementSummary,
  type RuleExpr,
  type UserAchievement,
  type UserMetric,
} from "../ipc";
import { t, tf } from "../i18n";

/** The metrics the rule model understands, with the label each one gets. */
const METRICS: { value: UserMetric; label: string }[] = [
  { value: "commits", label: "Commits" },
  { value: "pull_requests_merged", label: "Merged pull requests" },
  { value: "releases", label: "Releases" },
  { value: "streak_days", label: "Consecutive active days" },
];

const MAX_CONDITIONS = 50;
const MAX_DEPTH = 5;

function condition(metric: UserMetric, atLeast: number): RuleExpr {
  return { op: "condition", metric, at_least: atLeast };
}

/** A slug, because the id has to be one: `definition::validate` refuses anything else. */
function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40);
}

/** How deep a rule nests, so the page can warn before Rust refuses it. */
function depthOf(rule: RuleExpr): number {
  if (rule.op === "condition") return 1;
  return 1 + Math.max(0, ...rule.children.map(depthOf));
}

/** How many conditions a rule has in total. */
function conditionsOf(rule: RuleExpr): number {
  if (rule.op === "condition") return 1;
  return rule.children.reduce((sum, child) => sum + conditionsOf(child), 0);
}

export default function Builder() {
  const [builtins, setBuiltins] = useState<BuiltinAchievementSummary[]>([]);
  const [stored, setStored] = useState<UserAchievement[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState("");

  // The form state. Kept as a list even when the combinator is "all" so switching
  // between AND and OR does not lose the conditions the user typed.
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [id, setId] = useState("");
  const [idTouched, setIdTouched] = useState(false);
  const [combinator, setCombinator] = useState<"all" | "any">("all");
  const [rows, setRows] = useState<{ metric: UserMetric; atLeast: number }[]>([
    { metric: "commits", atLeast: 10 },
  ]);
  const [json, setJson] = useState("");
  const [jsonTouched, setJsonTouched] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [list, catalogue] = await Promise.all([listAchievements(), listBuiltinAchievements()]);
      setStored(list);
      setBuiltins(catalogue);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // The rule the form currently describes.
  const rule = useMemo<RuleExpr>(() => {
    const conditions = rows.map((r) => condition(r.metric, r.atLeast));
    const first = conditions[0];
    if (!first) return condition("commits", 1);
    if (conditions.length === 1) return first;
    return { op: combinator, children: conditions };
  }, [rows, combinator]);

  // The definition that will be sent: the JSON box wins once it is edited, because
  // that is the escape hatch for anything the form cannot express.
  const definition = useMemo(() => {
    if (jsonTouched) return json;
    return JSON.stringify(
      {
        id: id || slugify(name || "my-achievement"),
        name: name || "My Achievement",
        description,
        rule,
      },
      null,
      2,
    );
  }, [jsonTouched, json, id, name, description, rule]);

  // Show what the form produced, but never overwrite what the user typed.
  useEffect(() => {
    if (!jsonTouched) setJson(definition);
  }, [definition, jsonTouched]);

  // Local warnings. Rust refuses these too; saying so here saves a round trip and
  // explains the limit before the user hits it.
  const problems: string[] = [];
  const depth = depthOf(rule);
  const conditions = conditionsOf(rule);
  if (depth > MAX_DEPTH) problems.push(tf("The rule nests {depth} levels deep; the limit is {limit}.", { depth, limit: MAX_DEPTH }));
  if (conditions > MAX_CONDITIONS) problems.push(tf("The rule has {count} conditions; the limit is {limit}.", { count: conditions, limit: MAX_CONDITIONS }));
  if (builtins.some((b) => b.id === (id || slugify(name)))) {
    problems.push(t("That id belongs to a built-in achievement."));
  }
  if (stored.some((s) => s.id === (id || slugify(name)))) {
    problems.push(t("You already have an achievement with that id."));
  }

  const save = async () => {
    setError(null);
    setNotice("");
    try {
      const parsed: unknown = JSON.parse(definition);
      const savedId = await addAchievement(parsed);
      setNotice(tf("Saved {id}.", { id: savedId }));
      // Start a new definition: saving twice would only produce a duplicate-id error.
      setName("");
      setDescription("");
      setId("");
      setIdTouched(false);
      setRows([{ metric: "commits", atLeast: 10 }]);
      setJsonTouched(false);
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const remove = async (achievementId: string) => {
    setError(null);
    setNotice("");
    try {
      await removeAchievement(achievementId);
      setNotice(tf("Deleted {id}. An achievement it already unlocked stays unlocked.", { id: achievementId }));
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const updateRow = (index: number, patch: Partial<{ metric: UserMetric; atLeast: number }>) => {
    setRows((current) => current.map((row, i) => (i === index ? { ...row, ...patch } : row)));
    setJsonTouched(false);
  };

  return (
    <div className="page">
      <h2>{t("Achievement builder")}</h2>
      <p className="muted">
        {t("A user achievement is a rule, not code. It is judged against the same official statistics the built-in achievements use, at the moment a commit is ingested.")}
      </p>

      <h3>{t("New achievement")}</h3>

      <div className="row">
        <label className="field">
          <span>{t("Name")}</span>
          <input
            value={name}
            onChange={(e) => {
              setName(e.target.value);
              if (!idTouched) setId(slugify(e.target.value));
            }}
            placeholder={t("Ten commits in a day")}
          />
        </label>
        <label className="field">
          <span>{t("Id")}</span>
          <input
            value={id}
            onChange={(e) => {
              setId(slugify(e.target.value));
              setIdTouched(true);
            }}
            placeholder="ten-commits-in-a-day"
          />
        </label>
      </div>

      <label className="field">
        <span>{t("Description")}</span>
        <input value={description} onChange={(e) => setDescription(e.target.value)} />
      </label>

      <div className="row">
        <label className="field">
          <span>{t("Combine conditions with")}</span>
          <select
            value={combinator}
            onChange={(e) => {
              setCombinator(e.target.value as "all" | "any");
              setJsonTouched(false);
            }}
          >
            <option value="all">{t("all of them (AND)")}</option>
            <option value="any">{t("any of them (OR)")}</option>
          </select>
        </label>
      </div>

      <ul className="events" data-testid="builder-conditions">
        {rows.map((row, index) => (
          <li key={index}>
            <select
              value={row.metric}
              onChange={(e) => updateRow(index, { metric: e.target.value as UserMetric })}
            >
              {METRICS.map((m) => (
                <option key={m.value} value={m.value}>
                  {t(m.label)}
                </option>
              ))}
            </select>
            <span>{t("at least")}</span>
            <input
              type="number"
              min={1}
              value={row.atLeast}
              onChange={(e) => updateRow(index, { atLeast: Math.max(1, Number(e.target.value)) })}
            />
            <button
              onClick={() => {
                setRows((current) => current.filter((_, i) => i !== index));
                setJsonTouched(false);
              }}
            >
              {t("Remove")}
            </button>
          </li>
        ))}
      </ul>
      <div className="row">
        <button
          onClick={() => {
            setRows((current) => [...current, { metric: "commits", atLeast: 1 }]);
            setJsonTouched(false);
          }}
        >
          {t("Add condition")}
        </button>
      </div>

      <h3>{t("JSON")}</h3>
      <p className="muted">
        {t("The rule the form built, and the escape hatch: edit it directly for nesting the form cannot express. Saved rules are validated either way, so the limits hold.")}
      </p>
      <textarea
        className="json-editor"
        data-testid="builder-json"
        rows={14}
        value={json}
        onChange={(e) => {
          setJson(e.target.value);
          setJsonTouched(true);
        }}
      />

      {problems.length > 0 ? (
        <ul className="problems" data-testid="builder-problems">
          {problems.map((p) => (
            <li key={p}>{p}</li>
          ))}
        </ul>
      ) : null}

      <div className="row">
        <button disabled={problems.length > 0} onClick={() => void save()}>
          {t("Save achievement")}
        </button>
        <button
          onClick={() => {
            setJsonTouched(false);
            setError(null);
            setNotice("");
          }}
        >
          {t("Reset to the form")}
        </button>
      </div>

      {notice ? <p className="muted" data-testid="builder-notice">{notice}</p> : null}
      {error ? <p className="error" data-testid="builder-error">{error}</p> : null}

      <h3>{t("Your achievements")}</h3>
      {stored.length === 0 ? (
        <p className="muted">{t("None yet. The form above creates one, and it unlocks on the commit that satisfies it.")}</p>
      ) : (
        <ul className="events" data-testid="builder-stored">
          {stored.map((achievement) => (
            <li key={achievement.id}>
              <span className="kind">{achievement.name}</span>
              <span className="muted">{achievement.id}</span>
              <span className="mono">{JSON.stringify(achievement.rule)}</span>
              <button onClick={() => void remove(achievement.id)}>{t("Delete")}</button>
            </li>
          ))}
        </ul>
      )}

      <h3>{t("Built-in achievements")}</h3>
      <p className="muted">
        {t("A user achievement may not reuse one of these ids, and a built-in always wins a tie (spec 13.3).")}
      </p>
      <ul className="events">
        {builtins.map((b) => (
          <li key={b.id}>
            <span className="kind">{b.name}</span>
            <span className="muted">{b.description}</span>
            <span className="mono">{b.rarity}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

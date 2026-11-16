import { useCallback, useEffect, useState } from "react";
import { getRepositories, importHistory, type RepositoryEntry } from "../ipc";
import { dateTime } from "../format";

// Names are aliases: the storage layer never hands the real identifier to the UI
// (spec 26.2). Everything else on this page comes from the database.
export default function Repositories() {
  const [repos, setRepos] = useState<RepositoryEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [folder, setFolder] = useState<string>("");
  const [notice, setNotice] = useState<string>("");
  const [busy, setBusy] = useState(false);

  const load = useCallback(
    () =>
      getRepositories().then(setRepos, (e: unknown) =>
        setError(e instanceof Error ? e.message : String(e))),
    [],
  );

  useEffect(() => {
    void load();
  }, [load]);

  const runImport = async () => {
    if (!folder.trim()) return;
    setBusy(true);
    setNotice("Importing...");
    try {
      const report = await importHistory(folder.trim());
      setNotice(
        "Scanned " + report.scanned + ", imported " + report.ingested +
          ", already known " + report.duplicates,
      );
      await load();
    } catch (e) {
      setNotice("Import failed: " + (e instanceof Error ? e.message : String(e)));
    } finally {
      setBusy(false);
    }
  };

  if (error) return <p className="error">{error}</p>;
  if (!repos) return <p className="muted">Loading...</p>;

  return (
    <>
      <h2>Repositories</h2>
      <p className="muted">
        Names are stable aliases. The companion never displays the real path.
      </p>

      <h3>Import history</h3>
      <p className="muted">
        Point at a repository folder to import its past commits. Imports are silent
        and repeatable; already known commits are skipped.
      </p>
      <div className="row">
        <input
          value={folder}
          placeholder="C:\path\to\repo"
          onChange={(e) => setFolder(e.target.value)}
        />
        <button onClick={() => void runImport()} disabled={busy}>
          {busy ? "Working..." : "Import"}
        </button>
      </div>
      <p className="notice">{notice}</p>

      <h3>Known repositories</h3>
      {repos.length === 0 ? (
        <p className="muted">
          No repositories yet. Install the hooks, or import a folder above.
        </p>
      ) : (
        <ul className="list">
          {repos.map((repo) => (
            <li key={repo.alias} className="row spread">
              <span className="mono">{repo.alias}</span>
              <span className="muted">
                {repo.commits} commits · last {dateTime(repo.updatedAt)}
              </span>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

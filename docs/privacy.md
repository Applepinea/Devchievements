# Privacy Policy — Devchievements

**Last updated: 18 November 2026**

Devchievements is a local-first desktop application. It watches Git activity on your
own computer and turns it into achievements. This page describes, precisely, what
the application does with your information.

Contact: **csah1350@gmail.com**

---

## The short version

Devchievements does not collect, transmit, sell, or share your data. Everything it
records stays on your computer. There is no account, no sign-in, and no telemetry.

---

## What the application reads

Devchievements reads, **on your computer only**:

- the Git commits you make in repositories where you installed its Git hook;
- the author email and repository path of those commits;
- whether the commit is a merge or empty, so it can apply the same counting rules
  your Git statistics tools use.

It does **not** read your source code, your files, your browsing history, or your
keystrokes.

## What the application stores

Everything is stored in a single folder on your computer:

```text
%LOCALAPPDATA%\devchievements\
  companion.db      commit counts, achievements, notification history
  settings.json     your identity email, language, sound and popup settings
  spool\            records written by the Git hook before they are read
  assets\           images and sounds you imported yourself
  companion.log     a short log of what the application printed at start-up
```

You can open, copy, or delete this folder at any time. Deleting it removes
everything Devchievements knows, and the application will start fresh.

## What leaves your computer

**Nothing, by default.**

Devchievements makes a network request only when you explicitly ask it to check for
a newer version of the application. No commit data, repository name, email address,
or usage statistic is ever included in such a request. No update source is configured
in the version distributed through the Microsoft Store, so this build makes no
network requests at all.

There is no analytics service, no crash reporting service, and no advertising SDK in
this application.

## The Git hook

Devchievements works by installing a small hook into the `.git/hooks` folder of the
repositories you choose. That hook runs after each commit and hands a short record to
a helper program bundled with Devchievements. The hook is clearly marked, it never
blocks or alters a commit, and uninstalling Devchievements from its settings page
removes it.

## The browser extension, if you install it

The optional browser extension for GitHub reads the pages you are already looking at
on github.com — specifically, pull requests and releases you can see — and passes a
summary to the desktop application on your computer. It does not upload anything to a
server, and the desktop application works without it.

## Privacy Mode

Privacy Mode, available in Settings, replaces repository names in notification
popups with generic wording and hides commit messages. It also affects the diagnostic
export described below.

## Diagnostic export

When you export diagnostics to report a problem, the file is written to the folder
above and is **redacted by default**: repository names are replaced with stable
aliases and your user name is removed from paths. A raw form exists for cases where
the redacted one is not enough to diagnose the problem; it carries a warning in the
file itself, and you decide whether to share it.

## Children

Devchievements is a developer tool. It is not directed at children and collects no
personal information from anyone.

## Changes to this policy

If a future version changes any of the above, this page will be updated before that
version is released, and the change will be noted in the application's release notes.

## Contact

Questions about this policy: **csah1350@gmail.com**

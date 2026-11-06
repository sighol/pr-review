I want to create a user interface using Tauri and rust instead of just a TUI
like this repository has. The main view should be a side-by-side diff view with
nice syntax highlighting. For each line, I want to highlight on the line what
changed. Like, if a single word changed, I want the UI to show me that quickly.
I want to be able to drag in the line-number gutter on the lift side to create
comments for single line contexts or multiline contexts. When I drag multiple
lines, it should automatically create a suggestion with triple backticks with
the current code on as default.

When I add a comment, I should just be able to click in the gutter and write my
comment. No need to "submit" the comment to be added to my review. The backend
should store my drafts in a small sqlite database.

I want to have multiple tabs. First tab is the "summary". This is the PR title,
description, and all discussions that are not connected to a line. Next tab is
the main diff viewer. In the diff viewer we have a file picker side bar and a
two-way diff.

The third tab shows my review draft. Showing all the discussions that I have
pending draft comments on. At the end I have Then i can write a sentence and
then click on either approve, comment, or request-changes.







I think the file picker at the top is fine, actually. Let's discuss the tabs.

  It does not have to be tabs. What I need is an easy way to see the PR title, description, and non-code comments (main github PR page). Then I need the diff view.
  And finally I need some way to validate that all my pending comments are OK, a place to write the main PR review comment, and switch between comment, approve, and
  request-changes.

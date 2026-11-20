# Work in the desktop client

Work through Lore's daily loop in the desktop client: commit and push a change, read the history
and a diff, branch and merge with a conflict, and lock a file. By the end you'll have done in the
client everything the [quickstart](quickstart.md) does on the command line, and seen the client
refresh on its own when someone else pushes.

## Prerequisites

- The desktop client built, as [Build and run the desktop client](../how-to/build-and-run-the-desktop-client.md)
  describes.
- A local Lore Server running and a repository created in `~/my-project`, as steps 1 and 2 of the
  [quickstart](quickstart.md) describe.

## Steps

1. **Open the repository.**

   ```bash
   target/release/lore-desktop ~/my-project
   ```

   The tab is named after the directory. Under the toolbar the client shows the branch, `main`, its
   revision, and where it stands against the remote.

2. **Commit a file.** Create a file in the working tree:

   ```bash
   echo "Hello, Lore" > ~/my-project/hello.txt
   ```

   In the client, press **Scan**. `hello.txt` appears under **Not staged**. Press **Stage all**, and
   it moves to **Staged**. Type a message in **Commit message**, such as `Add hello`, and press
   **Commit**. The remote line now reads **Ahead of remote**.

3. **Push.** Press **Push**. A progress bar shows the upload, and the remote line reads
   **In sync with remote**.

4. **Read the history and a diff.** Change the file and commit again:

   ```bash
   echo "Hello again" >> ~/my-project/hello.txt
   ```

   Press **Scan**, click the `hello.txt` row to select it, and press **Diff**. A panel opens on the
   right with the working tree against the current revision: the added line is green on the right.
   Press **Stage 1 selected**, then commit with the message `Say hello again`.

   Press **History**. Both revisions are listed, newest first. Click the newest, then click
   `hello.txt` under **Files changed** to see what that revision changed against its parent.

5. **Branch and merge with a conflict.** Press **Branches**, type `feature` in the name field and
   press **Create**. The client switches to the new branch, and marks it with a dot. Change the file
   and commit it on `feature`:

   ```bash
   echo "Hello from feature" > ~/my-project/hello.txt
   ```

   In **Changes**, press **Scan**, **Stage all**, and commit. Back in **Branches**, select `main`
   and press **Switch**. Change the same file on `main` and commit it the same way:

   ```bash
   echo "Hello from main" > ~/my-project/hello.txt
   ```

   In **Branches**, select `feature` and press **Merge into current**. Both branches changed the
   same line, so the merge stops with a conflict: **Changes** lists `hello.txt` under **Unmerged**,
   and a notice reads **Merge in progress**. Press **Keep theirs (all)** to take `feature`'s version.
   The file moves to **Staged**; commit it to finish the merge.

   > [!TIP]
   > To back out of a merge instead, press **Abort merge…** beside the notice. The client asks
   > first, since aborting discards what the merge did.

6. **Lock a file.** Press **Locks**, type `hello.txt` in the path field and press **Lock**. The
   lock appears in the list, with who holds it and since when. Select it and press
   **Unlock 1 selected** to release it.

7. **Watch the client refresh itself.** Clone the repository into a second working tree from a
   terminal, then commit and push from it:

   ```bash
   cd ~
   lore clone lore://127.0.0.1:41337/my-project my-project-2
   cd ~/my-project-2
   echo "From the second tree" > second.txt
   lore stage second.txt
   lore commit "Add second"
   lore push
   ```

   Without your doing anything, the client's remote line changes to **Behind remote**. Press
   **Sync** to bring `second.txt` into `~/my-project`.

## Verify

The summary under the toolbar ends with `live`, and **History** lists the revision from the second
working tree. `~/my-project` holds both files:

```text
hello.txt
second.txt
```

## Troubleshooting

### The client doesn't refresh on its own

The summary reads **refreshes when the window gains focus** instead of `live`. The client couldn't
subscribe to the server's notifications, for example because it's offline; hover over the line for
the reason. Each time you return to the window it reads everything again, and **Refresh** reads it
at once.

### Stage all doesn't stage a new file

**Stage all** stages the changes Lore already knows about. Press **Scan** first, so a file changed
outside Lore is listed, then stage it.

## Next steps

- [Build and run the desktop client](../how-to/build-and-run-the-desktop-client.md) — open several
  instances, and find where the client keeps them.
- [Quickstart](quickstart.md) — the same loop on the command line.

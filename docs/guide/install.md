# Install

Before you start, [download the beta and check its checksum](./download.md).

## Install from the DMG

1. Double-click `StoneQL-0.1.0-beta-970ade2.dmg`. A window opens with **StoneQL** on the left and an **Applications** shortcut on the right.
2. Drag **StoneQL** onto **Applications**.
3. Close the window and eject the **StoneQL** disk (in Finder's sidebar, or drag it to the Trash).
4. Open **StoneQL** from your **Applications** folder, not from the DMG.

## Approve the first launch

Because the beta is not notarized, macOS blocks the first launch of a downloaded copy. You see a dialog titled **“StoneQL” Not Opened** (“Apple could not verify “StoneQL” is free of malware…”) with only two buttons, **Move to Trash** and **Done**. There is no Open button.

1. Choose **Done** (not Move to Trash).
2. Open **System Settings → Privacy & Security** and scroll down to the message about StoneQL.
3. Choose **Open Anyway**.
4. Confirm. macOS may ask for your login password.

From then on, open StoneQL from Applications as usual. Only approve a copy whose checksum matched.

StoneQL opens on a **Welcome to StoneQL** screen. Continue with [Connect to a database](./connect.md).

## Update to a newer build

1. Download the newer DMG and [check its checksum](./download.md#check-the-download).
2. Quit StoneQL.
3. Open the new DMG and drag **StoneQL** onto **Applications**, replacing the old copy.
4. Open StoneQL from Applications. A newly downloaded build may need the [first-launch approval](#approve-the-first-launch) again.

Your saved connections, groups, theme, grid preferences and saved passwords are kept. After an update, macOS asks once more before StoneQL can read a saved password; see [Keychain prompts](./troubleshooting.md#keychain-prompts).

## Uninstall

1. **Optional, to delete saved passwords:** in StoneQL, remove each connection that has a saved password (**Manage connections**, then remove the connection). Removing a connection deletes its saved password from the Keychain.
2. Quit StoneQL.
3. Move **StoneQL** from Applications to the Trash.

Moving the app to the Trash leaves your data behind, and a reinstalled StoneQL uses it again:

- Saved connections and preferences: `~/Library/WebKit/me.thixpin.stoneql` and `~/Library/Caches/me.thixpin.stoneql`.
- Trusted SSH host keys and the list of unknown outcomes: `~/Library/Application Support/me.thixpin.stoneql`.
- Saved passwords in your login Keychain. If you did not remove the connections first, open **Keychain Access** and delete the items named `me.thixpin.stoneql.credentials`.

To remove everything, delete those folders and Keychain items too.

# Download

## Requirements

- A Mac with **Apple Silicon** (M-series). There are no builds for Intel Macs, Windows or Linux yet.
- The beta was tested on **macOS 27.0.1**. Older macOS versions have not been tested.

## Get the beta

1. Open the release page for **[StoneQL v0.1.0-beta.1](https://github.com/stoneAce/stoneql/releases/tag/v0.1.0-beta.1)**. Later builds appear on the [releases page](https://github.com/stoneAce/stoneql/releases).
2. Under **Assets**, download both files into the same folder (for example Downloads):
   - `StoneQL-0.1.0-beta-970ade2.dmg`: the app.
   - `StoneQL-0.1.0-beta-970ade2.dmg.sha256`: its checksum.

## Check the download

The checksum proves the file you downloaded is exactly the file that was published.

1. Open **Terminal** (in Applications → Utilities).
2. Go to the folder with both files, for example:

   ```sh
   cd ~/Downloads
   ```

3. Run:

   ```sh
   shasum -a 256 -c StoneQL-0.1.0-beta-970ade2.dmg.sha256
   ```

4. It must print:

   ```
   StoneQL-0.1.0-beta-970ade2.dmg: OK
   ```

The expected SHA-256 is `3da858b6cd3c5819bd7d44ae5674cf257a28dfef58bdb6f0b7731c0ea9467a1a`. If the check fails, **do not open the file**: delete it and download it again.

## About the signature

The beta is signed ad hoc and is **not notarized by Apple** (it has no Developer ID signature yet). macOS therefore blocks its first launch until you approve it. [Install](./install.md) shows how.

Next: [Install StoneQL](./install.md).

/**
 * Puts a certificate thumbprint into the Tauri config, on a runner, for one
 * build.
 *
 * Windows code signing in Tauri is configured rather than passed: the bundler
 * reads `bundle > windows > certificateThumbprint` and calls `signtool` with
 * it. A thumbprint identifies one certificate in one machine's store, so
 * writing Brandon's into `tauri.conf.json` would break every build on every
 * machine that does not have that certificate installed, with an error about
 * a certificate rather than about the config. The committed config therefore
 * has no `bundle.windows` block at all, and this writes one just before the
 * build, from a thumbprint the runner learned by importing the `.pfx`.
 *
 * **With no thumbprint it does nothing and says so.** That is the state the
 * repository is in until Brandon adds the two secrets, and an unsigned
 * installer that builds is worth more than a signed one that cannot.
 *
 * The thumbprint is not itself a secret. It is the SHA-1 of the public
 * certificate and is printed on every signed binary; the `.pfx` and its
 * password are the secrets, and neither passes through here.
 *
 * Run: node scripts/windows-signing.mjs [<thumbprint>]
 * Or:  WINDOWS_CERTIFICATE_THUMBPRINT=... node scripts/windows-signing.mjs
 */
import { readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const CONFIG = join(root, "src-tauri", "tauri.conf.json");

/**
 * Where the countersignature comes from.
 *
 * Without one, every signature expires with the certificate and installers
 * built today stop verifying in a year. DigiCert's is free and needs no
 * account; `SIGNING_TIMESTAMP_URL` overrides it, because a timestamp server
 * being down is a normal Tuesday and it should not need a commit.
 */
const TIMESTAMP = process.env.SIGNING_TIMESTAMP_URL || "http://timestamp.digicert.com";

const thumbprint = (process.argv[2] || process.env.WINDOWS_CERTIFICATE_THUMBPRINT || "")
  .replace(/\s/g, "")
  .trim();

if (!thumbprint) {
  console.log(
    "no certificate thumbprint, so this build will not be signed.\n" +
      "Add WINDOWS_CERTIFICATE and WINDOWS_CERTIFICATE_PASSWORD as repository\n" +
      "secrets and the release workflow will start passing one.",
  );
  process.exit(0);
}

// A thumbprint is the certificate's SHA-1, so forty hex characters. Anything
// else is a secret that did not decode, and signtool's answer to that is to
// report no matching certificate, which reads like the import failed.
if (!/^[0-9a-fA-F]{40}$/.test(thumbprint)) {
  console.error(
    `not a certificate thumbprint: ${JSON.stringify(thumbprint)}.\n` +
      "Expected forty hex characters, which is what Import-PfxCertificate returns.",
  );
  process.exit(1);
}

const config = JSON.parse(readFileSync(CONFIG, "utf8"));

config.bundle.windows = {
  ...(config.bundle.windows ?? {}),
  certificateThumbprint: thumbprint.toUpperCase(),
  digestAlgorithm: "sha256",
  timestampUrl: TIMESTAMP,
};

// Trailing newline, so the file keeps the shape everything else in the repo
// has and a diff on a runner is one block rather than the whole file.
writeFileSync(CONFIG, `${JSON.stringify(config, null, 2)}\n`);

const written = JSON.parse(readFileSync(CONFIG, "utf8")).bundle.windows;

if (written?.certificateThumbprint !== thumbprint.toUpperCase()) {
  console.error("the thumbprint did not survive the write");
  process.exit(1);
}

console.log(
  `signing with certificate ${thumbprint.slice(0, 8).toUpperCase()}..., ` +
    `sha256, timestamped by ${TIMESTAMP}`,
);

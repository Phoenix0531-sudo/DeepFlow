// generate updates.json manifest + copy signed setup.exe + .sig to release dir.
// Usage:
//   node docs/scripts/gen-updates.js <version> <setup-path> <sig-path> <output-dir> <public-base-url>
//
// Output: <output-dir>/updates.json + <output-dir>/<setup-basename> + .sig
// (Have <public-base-url> pointing at your production CDN / GH releases path.)

const fs = require("fs");
const path = require("path");

function main() {
  const [version, setupPath, sigPath, outDir, publicBaseUrl] = process.argv.slice(2);
  if (!version || !setupPath || !sigPath || !outDir || !publicBaseUrl) {
    console.error("usage: gen-updates.js <version> <setup-path> <sig-path> <output-dir> <public-base-url>");
    process.exit(2);
  }
  fs.mkdirSync(outDir, { recursive: true });

  const setupBasename = path.basename(setupPath);
  const sigBasename = path.basename(sigPath);
  fs.copyFileSync(setupPath, path.join(outDir, setupBasename));
  fs.copyFileSync(sigPath,   path.join(outDir, sigBasename));

  const signature = fs.readFileSync(sigPath, "utf8").trim();
  const baseUrl = publicBaseUrl.replace(/\/+$/, "");
  const manifest = {
    version,
    notes: `DeepFlow ${version}`,
    pub_date: new Date().toISOString(),
    platforms: {
      "windows-x86_64": {
        signature,
        url: `${baseUrl}/${setupBasename}`,
      },
    },
  };
  fs.writeFileSync(
    path.join(outDir, "updates.json"),
    JSON.stringify(manifest, null, 2) + "\n",
    "utf8"
  );
  console.log("updates.json generated:");
  console.log("  version:", manifest.version);
  console.log("  signature length:", signature.length);
  console.log("  url:", manifest.platforms["windows-x86_64"].url);
  console.log("  setup copied:", setupBasename, fs.statSync(path.join(outDir, setupBasename)).size, "bytes");
}
main();
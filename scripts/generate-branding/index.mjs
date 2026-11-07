// Generates every product surface that cannot read branding/brand.json at runtime:
// Tauri bundle identity, the macOS Info.plist permission strings, and app icons.
// Usage: node scripts/generate-branding/index.mjs [--icons]
import { readFileSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const brand = JSON.parse(readFileSync(join(root, "branding/brand.json"), "utf8"));

for (const key of ["displayName", "productSlug", "publisher", "applicationId", "storageNamespace", "tagline"]) {
  if (typeof brand[key] !== "string" || brand[key].trim() === "") throw new Error(`brand.json: ${key} is required`);
}
if (brand.applicationId !== brand.storageNamespace) {
  // The Tauri identifier decides the data directory and credential namespace; keep them aligned.
  throw new Error("brand.json: applicationId and storageNamespace must match once released");
}
if (!/^[a-z0-9-]+$/.test(brand.productSlug)) throw new Error("brand.json: productSlug must be lowercase kebab-case");

const confPath = join(root, "src-tauri/tauri.conf.json");
const conf = JSON.parse(readFileSync(confPath, "utf8"));
conf.productName = brand.displayName;
conf.mainBinaryName = brand.productSlug;
conf.identifier = brand.applicationId;
conf.bundle.publisher = brand.publisher;
conf.bundle.shortDescription = brand.tagline;
conf.bundle.copyright = `© ${new Date().getFullYear()} ${brand.publisher}`;
writeFileSync(confPath, JSON.stringify(conf, null, 2) + "\n");

const xml = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const name = xml(brand.displayName);
writeFileSync(
  join(root, "src-tauri/Info.plist"),
  `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>NSMicrophoneUsageDescription</key>
  <string>${name} records your voice only while you dictate, and transcribes it on this Mac.</string>
  <key>LSUIElement</key>
  <false/>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
`,
);

if (process.argv.includes("--icons")) {
  execFileSync("npx", ["-y", "@tauri-apps/cli@2.12.4", "icon", join(root, brand.iconSource), "-o", join(root, "src-tauri/icons")], { stdio: "inherit" });
}
console.log(`branding: ${brand.displayName} (${brand.applicationId})`);

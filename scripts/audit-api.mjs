/**
 * What Raycast declares, against what Sill draws and answers.
 *
 * The other audits ask about extensions. `audit-extensions.mjs` runs real
 * commands and ranks what they needed and did not get; this asks the question
 * before any extension is involved: of everything the API declares, how much
 * of it has somewhere to land in this launcher?
 *
 * The two answer different halves and both are needed. Usage tells you what to
 * do next, and it can only ever report on the extensions that were run. This
 * reports the whole surface, including the parts no extension in the sample
 * happened to touch, which is where "it works for the ten we tried" turns into
 * "it works".
 *
 * ```text
 * node scripts/audit-api.mjs            # the summary
 * node scripts/audit-api.mjs --props    # every component, prop by prop
 * ```
 *
 * Not part of `npm run verify`, and it cannot be: it reads `@raycast/api`'s own
 * type declarations, which arrive with the sparse checkout rather than with
 * this repository. Run `npm run extensions:fetch` first.
 *
 * ## What "read" means here, and what it does not
 *
 * A prop counts as read when its name appears in the window's renderer or the
 * host's API layer. That is a **floor and deliberately a crude one**: it
 * over-reports, because a name can appear in a comment, and it cannot see a
 * prop read through a variable. The alternative is a second implementation of
 * the renderer's own logic, which would agree with itself and with nothing on
 * screen.
 *
 * So a prop this calls missing is missing. A prop it calls read may still be
 * read wrongly, and only running a real extension shows that. The same
 * reasoning `store::capability` is written down with, for the same reason.
 */
import { readFileSync, existsSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { createRequire } from "node:module";

const root = resolve(import.meta.dirname, "..");
const require_ = createRequire(import.meta.url);
const ts = require_("typescript");

const showProps = process.argv.includes("--props");

/**
 * Raycast's own declarations, wherever the checkout put them.
 *
 * Every fetched extension carries a copy under its `node_modules`, and they
 * are the same file. The first one found is taken rather than a version being
 * pinned here: what is wanted is the API the extensions in the checkout are
 * written against, which is exactly what they installed.
 */
function declarations() {
  const home = join(root, "extensions", "raycast-src", "extensions");
  if (!existsSync(home)) return null;

  for (const extension of readdirSync(home)) {
    const dts = join(home, extension, "node_modules", "@raycast", "api", "types", "index.d.ts");
    if (existsSync(dts)) return dts;
  }
  return null;
}

const dts = declarations();
if (!dts) {
  console.error(
    "No @raycast/api declarations found.\n" +
      "They arrive with the extensions the view gate draws:\n" +
      "  npm run extensions:fetch\n" +
      "and are installed by the first `npm install` inside one of them.",
  );
  process.exit(1);
}

const source = ts.createSourceFile(
  dts,
  readFileSync(dts, "utf8"),
  ts.ScriptTarget.Latest,
  true,
);

// ---------------------------------------------------------------- the surface

/**
 * Every interface and type alias in the file, by the name it was declared as.
 *
 * The declarations are one flattened bundle, so a name collision between two
 * components becomes `ItemProps`, `ItemProps_2`, `ItemProps_3`. Nothing here
 * tries to undo that: the namespaces below say which is which, by pointing at
 * one, and that pointer is the only reliable way round.
 */
const declared = new Map();

for (const statement of source.statements) {
  if (ts.isInterfaceDeclaration(statement) || ts.isTypeAliasDeclaration(statement)) {
    declared.set(statement.name.text, statement);
  }
}

/** The members of an interface, following what it extends. */
function membersOf(name, seen = new Set()) {
  if (seen.has(name)) return [];
  seen.add(name);

  const node = declared.get(name);
  if (!node) return [];

  // `type Props = SomethingProps` is a pointer; follow it.
  if (ts.isTypeAliasDeclaration(node)) {
    return ts.isTypeReferenceNode(node.type) ? membersOf(node.type.typeName.getText(), seen) : [];
  }

  const own = node.members
    .filter((member) => ts.isPropertySignature(member) && member.name)
    .map((member) => member.name.getText());

  const inherited = (node.heritageClauses ?? []).flatMap((clause) =>
    clause.types.flatMap((type) => membersOf(type.expression.getText(), seen)),
  );

  return [...new Set([...own, ...inherited])];
}

/**
 * Every component the API declares, by the name an extension writes.
 *
 * Walked from the namespaces rather than from the `declare const`s, because
 * the namespace nesting is what says a thing is called `List.Dropdown.Item`.
 * A namespace carrying `export type Props = X` is a component; one carrying
 * only other namespaces is a container and is not counted as a thing to draw.
 */
const components = new Map();

function walkNamespaces(node, path) {
  if (!ts.isModuleDeclaration(node) || !node.body || !ts.isModuleBlock(node.body)) return;

  const here = [...path, node.name.text];

  for (const statement of node.body.statements) {
    if (
      ts.isTypeAliasDeclaration(statement) &&
      statement.name.text === "Props" &&
      ts.isTypeReferenceNode(statement.type)
    ) {
      components.set(here.join("."), membersOf(statement.type.typeName.getText()));
    }
    walkNamespaces(statement, here);
  }
}

for (const statement of source.statements) walkNamespaces(statement, []);

/** Every enum the API declares, and its members. */
const enums = new Map();

for (const statement of source.statements) {
  if (!ts.isEnumDeclaration(statement)) continue;
  enums.set(statement.name.text, statement.members.map((member) => member.name.getText()));
}

// ------------------------------------------------------- what Sill does with it

/**
 * Everywhere a prop or a name could be read.
 *
 * The window's renderer and the host's API layer, which between them are the
 * whole of what an extension can reach. Read once and searched as one string:
 * which file reads a prop is a question for somebody fixing it, not for a
 * count of what is missing.
 */
function readable() {
  const source = (f) => (f.endsWith(".svelte") || f.endsWith(".ts")) && !f.includes(".test.");

  const files = [
    // `.ts` beside the components as well as the components, because the icon
    // table lives in `marks.ts`. It was `.svelte` only, and moving that table
    // out of `ExtIcon.svelte` took the Icon enum from 106 of 469 to 40
    // without a single name changing.
    ...walk(join(root, "src", "lib", "components")).filter(source),
    ...walk(join(root, "src", "lib", "exthost")).filter(source),
    ...walk(join(root, "host", "src")).filter((f) => f.endsWith(".ts")),
    join(root, "src", "routes", "+page.svelte"),
  ];

  return files
    .filter(existsSync)
    .map((file) => readFileSync(file, "utf8"))
    .join("\n");
}

function walk(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    return entry.isDirectory() ? walk(path) : [path];
  });
}

const sill = readable();

/**
 * Whether a name is read anywhere, as a word rather than as a substring.
 *
 * `id` matches almost any file as a substring and almost never as a prop, so
 * the boundaries matter more here than the search does.
 */
function reads(name) {
  return new RegExp(`\\b${name.replace(/[^\w]/g, "")}\\b`).test(sill);
}

// ------------------------------------------------------------------- the report

const drawn = [];
const partly = [];
const missing = [];

for (const [name, props] of [...components].sort()) {
  // A component is drawn if the window mentions its tag at all. The renderer
  // matches tags by their last segment, so that is what is asked for.
  const tag = name.split(".").pop();
  const known = new RegExp(`["'\`]${name}["'\`]|["'\`]${tag}["'\`]`).test(sill);

  const unread = props.filter((prop) => !reads(prop));

  if (!known) missing.push({ name, props });
  else if (unread.length) partly.push({ name, props, unread });
  else drawn.push({ name, props });
}

const say = (n, of) => `${n} of ${of} (${of === 0 ? 0 : Math.round((n / of) * 100)}%)`;

console.log(`Raycast's declarations: ${dts.replace(root, ".")}\n`);
console.log("components");
console.log(`  declared:            ${components.size}`);
console.log(`  every prop read:     ${say(drawn.length, components.size)}`);
console.log(`  drawn, props missing:${say(partly.length, components.size)}`);
console.log(`  not drawn at all:    ${say(missing.length, components.size)}`);

if (missing.length) {
  console.log("\ncomponents with no tag anywhere in the launcher");
  for (const { name, props } of missing) {
    console.log(`  ${name.padEnd(34)} ${props.length} prop(s)`);
  }
}

if (partly.length) {
  console.log("\ncomponents drawn with props nothing reads");
  for (const { name, unread, props } of partly) {
    console.log(
      `  ${name.padEnd(34)} ${String(unread.length).padStart(2)} of ${props.length} unread` +
        (showProps ? `: ${unread.join(", ")}` : ""),
    );
  }
  if (!showProps) console.log("\n  (--props to name them)");
}

console.log("\nenums, by how many members have somewhere to land");
for (const [name, members] of [...enums].sort()) {
  const read = members.filter((member) => reads(member));
  console.log(`  ${name.padEnd(24)} ${say(read.length, members.length)}`);
}

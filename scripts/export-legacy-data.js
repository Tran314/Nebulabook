/*
 * One-time, READ-ONLY migration helper for the OLD Nebula browser application.
 *
 * 1. Open the old app at the exact origin/browser profile that owns your notes.
 * 2. Open Developer Tools → Console, inspect this script, then run it deliberately.
 * 3. Keep the downloaded JSON. Import it through the native app's Import dialog.
 *
 * The native app cannot automatically read a browser's IndexedDB. This helper
 * reads every object store in NebulaLocalDB in one readonly transaction. It never
 * deletes records, clears stores, changes schema, signs in, or uploads anything.
 * Browser downloads still require your browser's normal permission/confirmation.
 */
(async function exportLegacyNebulaData() {
  "use strict";
  const databaseName = "NebulaLocalDB";
  let database;

  function assertJsonSafe(value, seen = new WeakSet()) {
    if (value === null || typeof value === "string" || typeof value === "boolean") return;
    if (typeof value === "number" && Number.isFinite(value)) return;
    if (typeof value !== "object") {
      throw new Error("A legacy record contains a value JSON cannot preserve. Export stopped; original data is unchanged.");
    }
    if (seen.has(value)) throw new Error("A legacy record contains a circular reference. Export stopped; original data is unchanged.");
    if (value instanceof Date) {
      if (Number.isNaN(value.getTime())) throw new Error("A legacy record contains an invalid date.");
      return;
    }
    if (!Array.isArray(value) && Object.getPrototypeOf(value) !== Object.prototype && Object.getPrototypeOf(value) !== null) {
      throw new Error("A legacy record contains binary or unsupported structured data. Export stopped rather than silently discard it.");
    }
    seen.add(value);
    for (const child of Object.values(value)) assertJsonSafe(child, seen);
    seen.delete(value);
  }

  try {
    if (!globalThis.indexedDB || typeof indexedDB.databases !== "function") {
      throw new Error("This browser cannot list IndexedDB databases safely. Run the helper in a current browser containing the old app's notes.");
    }
    const databases = await indexedDB.databases();
    if (!databases.some(entry => entry.name === databaseName)) {
      throw new Error("NebulaLocalDB was not found at this origin. Open the original app URL in the original browser profile. No database was created.");
    }

    database = await new Promise((resolve, reject) => {
      const request = indexedDB.open(databaseName); // No version: never requests an upgrade.
      request.onupgradeneeded = () => {
        // The database may disappear after databases() resolves. Abort a would-be
        // creation instead of leaving a new empty database behind.
        request.transaction.abort();
      };
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error || new Error("Could not open the existing legacy database."));
      request.onblocked = () => console.warn("Close other old Nebula tabs if the read-only export remains blocked.");
    });

    const names = Array.from(database.objectStoreNames);
    if (!names.includes("notes")) throw new Error("This is not a supported Nebula database: the notes store is missing.");
    const stores = await new Promise((resolve, reject) => {
      const transaction = database.transaction(names, "readonly");
      const records = Object.create(null);
      transaction.oncomplete = () => resolve(records);
      transaction.onerror = () => reject(transaction.error || new Error("Reading the legacy database failed."));
      transaction.onabort = () => reject(transaction.error || new Error("Reading the legacy database was interrupted."));
      for (const name of names) {
        const request = transaction.objectStore(name).getAll();
        request.onsuccess = () => { records[name] = request.result; };
      }
    });
    assertJsonSafe(stores);
    const exportedAt = new Date().toISOString();
    const data = {
      format: "nebula-legacy-indexeddb",
      schemaVersion: 1,
      exportedAt,
      database: { name: database.name, version: database.version },
      stores,
    };
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: "application/json;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `nebula-legacy-${exportedAt.replace(/[:.]/g, "-")}.json`;
    document.documentElement.appendChild(link);
    link.click();
    link.remove();
    setTimeout(() => URL.revokeObjectURL(url), 60000);
    console.info(`Prepared a local JSON download with ${stores.notes.length} notes and ${names.length} stores (${blob.size} bytes). Check your Downloads folder; keep this original backup. Nothing was uploaded, modified, or deleted.`);
    if (blob.size > 64 * 1024 * 1024) {
      console.warn("This backup exceeds the native app's 64 MiB import limit. Preserve it and split a COPY into smaller validated imports; do not delete the original browser data.");
    }
  } catch (error) {
    console.error("Nebula export failed. Original browser data is unchanged:", error);
  } finally {
    if (database) database.close();
  }
})();

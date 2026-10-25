// Keep the original evaluation entry point; it now tests the approved vendored source.
await import('./check-sqlx-vendor.mjs');
console.log('Approved SQLx source verified; no registry cache or generated copy modified.');

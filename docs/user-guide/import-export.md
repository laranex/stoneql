# Import and export

All transfers are **simulated**. No dump, compression or filesystem backup is created.

## Export

Export From selects the current table or database. Quick mode always uses Structure + Data. Custom mode exposes table selection, Structure/Data choices and advanced engine-specific SQL options. The previous Custom type is retained when toggling modes, but does not affect Quick exports.

SQL, CSV and JSON have format-specific validation. CSV has no schema representation: use Custom → Data Only when required by validation. SQL options include supported DDL, insert style, transaction and object settings according to the engine. ZIP/GZIP selections affect only the simulated summary. Filename and destination controls describe a future local output, not a written file.

Progress supports cancellation and simulated errors. Success summaries report selected tables, records, format and configuration; they are not backup evidence.

## Import

CSV input is parsed and schema-validated before atomic mock insertion. Invalid rows must not partially mutate mock state. SQL import does not execute a real script. File selection and progress are prototype interactions. XLSX streaming, real SQL execution, compression, filesystem access and durable jobs are planned in the [transfer architecture](../architecture/import-export.md).

# Roadmap

## 1. Data foundations

- Structured JSON preserves integer precision.
- Record identifiers remain deterministic across processes.
- Dates round-trip at millisecond precision.
- Library tests run without credentials or network services.

## 2. Local agent activity

- Read agent activity and token usage from local records.
- Query supported clients for remaining usage.
- Keep account failures isolated by data source.
- Validate parsers with synthetic fixtures.

## 3. macOS experience

- Present agent activity and usage in the menu bar and notch panel.
- Provide usage statistics and local display settings.
- Build a standalone application without a developer account.

## 4. Reproducible builds

- Build and test from a clean checkout.
- Include all required application resources and license notices.
- Validate source and package boundaries in continuous integration.
- Document supported clients, data access, and local storage.

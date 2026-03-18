-- Rulest's own architecture rules (dogfooding)

INSERT INTO ownership_rules (crate_name, description, kind) VALUES ('rulest-core', 'Owns registry schema, models, queries, and advisory types', 'must_own');
INSERT INTO ownership_rules (crate_name, description, kind) VALUES ('rulest-core', 'No I/O or filesystem operations', 'must_not');
INSERT INTO ownership_rules (crate_name, description, kind) VALUES ('rulest-indexer', 'Owns source code parsing and sync logic', 'must_own');
INSERT INTO ownership_rules (crate_name, description, kind) VALUES ('rulest-mcp', 'Owns MCP protocol handling', 'must_own');
INSERT INTO ownership_rules (crate_name, description, kind) VALUES ('rulest-cli', 'Owns CLI argument parsing and command dispatch', 'must_own');
INSERT INTO ownership_rules (crate_name, description, kind) VALUES ('domain', 'No infrastructure concerns', 'must_not');

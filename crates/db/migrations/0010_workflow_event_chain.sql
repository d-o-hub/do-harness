-- Tamper-evident hash chain for workflow event log.
ALTER TABLE workflow_events ADD COLUMN seq INTEGER;
ALTER TABLE workflow_events ADD COLUMN chain_hash TEXT;

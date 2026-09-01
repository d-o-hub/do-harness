-- Add hash chain columns to workflow_events log.
ALTER TABLE workflow_events ADD COLUMN seq INTEGER;
ALTER TABLE workflow_events ADD COLUMN chain_hash TEXT;

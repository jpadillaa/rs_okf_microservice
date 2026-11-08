-- Plan 3, I2: orden de llegada exacto. created_at tiene resolución de segundos y el
-- identificador es aleatorio, así que no ordenan dos subidas del mismo segundo.
ALTER TABLE jobs ADD COLUMN arrival BIGSERIAL;
DROP INDEX jobs_document_key;
CREATE INDEX jobs_document_key ON jobs(document_key, arrival) WHERE document_key IS NOT NULL;

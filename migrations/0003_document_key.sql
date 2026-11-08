-- Plan 3, I2: clave opaca con que el sistema de origen agrupa las versiones de un documento.
ALTER TABLE jobs ADD COLUMN document_key TEXT;
CREATE INDEX jobs_document_key ON jobs(document_key, created_at, id) WHERE document_key IS NOT NULL;

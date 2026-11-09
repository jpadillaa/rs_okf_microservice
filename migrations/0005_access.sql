-- Plan 3, I7: permisos recibidos con el original (JSON opaco), copiados sin cambios al bundle.
ALTER TABLE jobs ADD COLUMN access TEXT;

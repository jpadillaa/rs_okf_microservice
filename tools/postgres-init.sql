-- Base desechable para las pruebas; la aplicación utiliza POSTGRES_DB.
SELECT 'CREATE DATABASE okf_test'
WHERE NOT EXISTS (SELECT 1 FROM pg_database WHERE datname = 'okf_test')
\gexec

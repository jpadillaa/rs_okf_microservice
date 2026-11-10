//! Contrato OpenAPI generado de los handlers y Swagger UI embebido en el binario.
use utoipa::OpenApi;
use utoipa_swagger_ui::{Config, SwaggerUi};

/// Documento que se enviará como archivo, con nombre y extensión originales.
#[derive(utoipa::ToSchema)]
#[allow(dead_code)] // Describe Multipart; Axum recibe los bytes directamente.
pub(crate) struct Upload {
    /// **Único campo obligatorio.** Selecciona un archivo .pdf, .docx, .xlsx o .pptx
    /// y pulsa **Execute** para iniciar la conversión.
    #[schema(value_type = String, format = Binary)]
    file: Vec<u8>,
    /// **Opcional: déjalo vacío para convertir un archivo.** Úsalo solo si quieres
    /// agrupar versiones de un documento con una misma clave, como `manual-equipo`.
    /// Si lo rellenas, admite de 1 a 256 caracteres ASCII visibles, sin espacios.
    #[schema(
        nullable = false,
        example = "",
        min_length = 1,
        max_length = 256,
        pattern = "^[!-~]+$"
    )]
    document_key: Option<String>,
    /// **Opcional: déjalo vacío para convertir un archivo.** Úsalo solo si otro sistema
    /// proporciona permisos en JSON que quieras conservar en el resultado.
    /// El servicio conserva ese JSON; no lo aplica como control de acceso.
    #[schema(nullable = false, example = "")]
    access: Option<String>,
}

#[derive(utoipa::ToSchema)]
#[schema(value_type = String, format = Binary)]
#[allow(dead_code)] // Esquema de la respuesta binaria, sin cambiar el handler.
pub(crate) struct BinaryFile(Vec<u8>);

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Microservicio OKF — API de pruebas",
        description = "Convierte PDF, DOCX, XLSX y PPTX en bundles OKF.\n\n\
            1. Abre **POST /jobs**, selecciona un archivo en **file** y pulsa **Execute**. \
            **Solo el archivo es obligatorio**: deja **document_key** y **access** vacíos, \
            con **Send empty value** desmarcado.\n\
            2. Copia el **job_id** de la respuesta en **GET /jobs/{id}** y consulta hasta obtener **complete** o **partial**.\n\
            3. Usa ese identificador en **GET /jobs/{id}/bundle** para descargar el ZIP.\n\n\
            La API y el worker deben estar iniciados. **GET /health** permite comprobarlo."
    ),
    paths(crate::submit, crate::versions, crate::status, crate::download, crate::observability::health, crate::observability::metrics),
    tags(
        (name = "Trabajos", description = "Subir un documento, consultar la conversión y descargar el resultado."),
        (name = "Operación", description = "Salud del servicio y métricas.")
    )
)]
struct ApiDoc;

pub(crate) fn swagger_ui() -> axum::Router<crate::App> {
    let config = Config::from("/api-docs/openapi.json")
        .try_it_out_enabled(true)
        .display_request_duration(true)
        // Evita enviar el contrato a un validador externo.
        .validator_url("none");
    // Conserva el inicializador y la configuración de la biblioteca; solo añade
    // el plugin local que desmarca el envío de campos opcionales vacíos al abrirlos.
    let file = utoipa_swagger_ui::serve(
        "swagger-initializer.js",
        std::sync::Arc::new(config.clone()),
    )
    .expect("inicializador Swagger válido")
    .expect("inicializador Swagger embebido");
    let initializer = String::from_utf8(file.bytes.into_owned()).expect("inicializador UTF-8");
    let plugin_slot = "SwaggerUIBundle.plugins.DownloadUrl";
    assert_eq!(initializer.matches(plugin_slot).count(), 1);
    let initializer = initializer.replacen(
        plugin_slot,
        &format!(
            "{plugin_slot},\n{}",
            include_str!("swagger-empty-fields.js")
        ),
        1,
    );
    axum::Router::new()
        .merge(
            SwaggerUi::new("/docs")
                .url("/api-docs/openapi.json", ApiDoc::openapi())
                .config(config),
        )
        .route(
            "/docs/swagger-initializer.js",
            axum::routing::get(move || {
                let initializer = initializer.clone();
                async move {
                    (
                        [(
                            axum::http::header::CONTENT_TYPE,
                            "text/javascript; charset=utf-8",
                        )],
                        initializer,
                    )
                }
            }),
        )
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn swagger_serves_local_assets_and_the_documented_http_contract() {
        let tmp = tempfile::tempdir().unwrap();
        let mut state = crate::app(tmp.path().to_path_buf()).await.unwrap();
        std::sync::Arc::make_mut(&mut state.cfg).swagger_enabled = true;
        let app = crate::router(state);
        let request = |path| Request::builder().uri(path).body(Body::empty()).unwrap();
        let redirect = app.clone().oneshot(request("/docs")).await.unwrap();
        assert!(redirect.status().is_redirection());
        assert_eq!(redirect.headers()["location"], "/docs/");

        for (path, mime, marker) in [
            ("/docs/", "text/html", "./swagger-ui-bundle.js"),
            ("/docs/swagger-ui.css", "text/css", ".swagger-ui"),
            (
                "/docs/swagger-ui-bundle.js",
                "javascript",
                "SwaggerUIBundle",
            ),
            (
                "/docs/swagger-ui-standalone-preset.js",
                "javascript",
                "SwaggerUIStandalonePreset",
            ),
            (
                "/docs/swagger-initializer.js",
                "javascript",
                "/api-docs/openapi.json",
            ),
        ] {
            let response = app.clone().oneshot(request(path)).await.unwrap();
            assert_eq!(response.status(), 200, "{path}");
            assert!(
                response.headers()["content-type"]
                    .to_str()
                    .unwrap()
                    .contains(mime)
            );
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let text = std::str::from_utf8(&bytes).unwrap();
            assert!(text.contains(marker), "{path}");
            if path.ends_with("swagger-initializer.js") {
                assert!(text.contains("\"validatorUrl\": \"none\""));
                assert!(text.contains("\"tryItOutEnabled\": true"));
            }
        }

        let response = app
            .clone()
            .oneshot(request("/api-docs/openapi.json"))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["content-type"], "application/json");
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let doc: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let paths = &doc["paths"];
        assert_eq!(paths.as_object().unwrap().len(), 5);
        for (path, method, code, mime) in [
            ("/jobs", "post", "202", "application/json"),
            ("/jobs", "get", "200", "application/json"),
            ("/jobs/{id}", "get", "200", "application/json"),
            ("/jobs/{id}/bundle", "get", "200", "application/zip"),
            ("/health", "get", "503", "application/json"),
            ("/metrics", "get", "200", "text/plain"),
        ] {
            assert!(
                paths[path][method]["responses"][code]["content"][mime].is_object(),
                "{path}"
            );
        }
        let upload = &doc["components"]["schemas"]["Upload"];
        assert_eq!(upload["required"], serde_json::json!(["file"]));
        assert_eq!(upload["properties"]["document_key"]["type"], "string");
        assert_eq!(upload["properties"]["access"]["type"], "string");
        let list = &paths["/jobs"]["get"]["parameters"][0];
        assert_eq!(
            (&list["name"], &list["in"]),
            (&"document_key".into(), &"query".into())
        );
        assert_eq!(list["required"], true);
        assert_eq!(upload["properties"]["file"]["type"], "string");
        assert_eq!(upload["properties"]["file"]["format"], "binary");
        let body = &paths["/jobs"]["post"]["requestBody"];
        assert_eq!(body["required"], true);
        assert_eq!(
            body["content"]["multipart/form-data"]["schema"]["$ref"],
            "#/components/schemas/Upload"
        );
        assert_eq!(
            paths["/jobs/{id}/bundle"]["get"]["responses"]["200"]["content"]["application/zip"]["schema"]
                ["format"],
            "binary"
        );

        // Las rutas documentales conviven con los handlers que extraen App.
        // Sin worker, la respuesta real de salud sigue siendo 503 y tiene el esquema documentado.
        let response = app.oneshot(request("/health")).await.unwrap();
        assert_eq!(response.status(), 503);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let health: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(health["worker"], false);
        let properties = doc["components"]["schemas"]["Health"]["properties"]
            .as_object()
            .unwrap();
        assert_eq!(health.as_object().unwrap().len(), properties.len());
        assert!(
            health
                .as_object()
                .unwrap()
                .keys()
                .all(|key| properties.contains_key(key))
        );
    }

    #[tokio::test]
    async fn disabled_swagger_hides_ui_assets_and_contract_but_keeps_the_api() {
        let tmp = tempfile::tempdir().unwrap();
        let mut state = crate::app(tmp.path().to_path_buf()).await.unwrap();
        std::sync::Arc::make_mut(&mut state.cfg).swagger_enabled = false;
        let app = crate::router(state);
        for path in [
            "/docs",
            "/docs/",
            "/docs/swagger-ui.css",
            "/docs/swagger-ui-bundle.js",
            "/docs/swagger-ui-standalone-preset.js",
            "/docs/swagger-initializer.js",
            "/api-docs/openapi.json",
        ] {
            let request = Request::builder().uri(path).body(Body::empty()).unwrap();
            let response = app.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), 404, "{path}");
            assert!(!response.headers().contains_key("location"), "{path}");
        }
        for (path, status, content) in [
            ("/health", 503, "\"database\":true"),
            ("/metrics", 200, "okf_jobs"),
            ("/jobs/nonexistent", 404, "trabajo inexistente"),
            ("/jobs/nonexistent/bundle", 404, "trabajo inexistente"),
        ] {
            let request = Request::builder().uri(path).body(Body::empty()).unwrap();
            let response = app.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), status, "{path}");
            let body = response.into_body().collect().await.unwrap().to_bytes();
            assert!(
                std::str::from_utf8(&body).unwrap().contains(content),
                "{path}"
            );
        }
    }
}

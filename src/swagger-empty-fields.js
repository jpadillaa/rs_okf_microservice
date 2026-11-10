// Swagger UI incluye los campos opcionales vacíos por defecto. Cambia solo la
// selección inicial: el usuario puede seguir marcando la casilla expresamente.
function EmptyOptionalFieldsPlugin(system) {
  return {
    wrapComponents: {
      ParameterIncludeEmpty: (Original) => (props) =>
        system.React.createElement(Original, {
          ...props,
          isIncludedOptions: {
            ...props.isIncludedOptions,
            defaultValue: false,
          },
        }),
    },
  };
}

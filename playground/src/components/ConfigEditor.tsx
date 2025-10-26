import { useEffect, useState } from "preact/hooks";
import { ConfigFieldInfo } from "../types";

interface ConfigEditorProps {
  fields: ConfigFieldInfo[];
  value: Record<string, unknown>;
  onChange: (value: Record<string, unknown>) => void;
}

export function ConfigEditor({ fields, value, onChange }: ConfigEditorProps) {
  const [config, setConfig] = useState<Record<string, unknown>>(value);

  useEffect(() => {
    setConfig(value);
  }, [value]);

  const handleFieldChange = (fieldName: string, fieldValue: unknown) => {
    const newConfig = { ...config, [fieldName]: fieldValue };
    setConfig(newConfig);
    onChange(newConfig);
  };

  const renderField = (field: ConfigFieldInfo) => {
    const currentValue = config[field.name];

    const typeName = field.type_name.toLowerCase();

    if (
      typeName.includes("option<vec<string>>") ||
      typeName.includes("vec<string>")
    ) {
      const stringValue = Array.isArray(currentValue)
        ? currentValue.join(", ")
        : "";
      return (
        <div key={field.name} class="config-field">
          <label>
            <span class="field-name">{field.name}</span>
            {field.doc.length > 0 && (
              <span class="field-doc" title={field.doc.join("\n")}>
                {field.doc.join(" ")}
              </span>
            )}
          </label>
          <input
            type="text"
            value={stringValue}
            placeholder="Comma-separated values"
            onInput={(e) => {
              const text = (e.target as HTMLInputElement).value.trim();
              const arr = text ? text.split(",").map((s) => s.trim()) : [];
              handleFieldChange(field.name, arr.length > 0 ? arr : null);
            }}
          />
        </div>
      );
    }

    if (typeName.includes("option<string>") || typeName.includes("string")) {
      return (
        <div key={field.name} class="config-field">
          <label>
            <span class="field-name">{field.name}</span>
            {field.doc.length > 0 && (
              <span class="field-doc" title={field.doc.join("\n")}>
                {field.doc.join(" ")}
              </span>
            )}
          </label>
          <input
            type="text"
            value={(currentValue as string) || ""}
            onInput={(e) =>
              handleFieldChange(
                field.name,
                (e.target as HTMLInputElement).value || null,
              )}
          />
        </div>
      );
    }

    if (typeName.includes("bool")) {
      return (
        <div key={field.name} class="config-field">
          <label>
            <input
              type="checkbox"
              checked={!!currentValue}
              onChange={(e) =>
                handleFieldChange(
                  field.name,
                  (e.target as HTMLInputElement).checked,
                )}
            />
            <span class="field-name">{field.name}</span>
            {field.doc.length > 0 && (
              <span class="field-doc" title={field.doc.join("\n")}>
                {field.doc.join(" ")}
              </span>
            )}
          </label>
        </div>
      );
    }

    if (typeName.includes("i32") || typeName.includes("i64")) {
      return (
        <div key={field.name} class="config-field">
          <label>
            <span class="field-name">{field.name}</span>
            {field.doc.length > 0 && (
              <span class="field-doc" title={field.doc.join("\n")}>
                {field.doc.join(" ")}
              </span>
            )}
          </label>
          <input
            type="number"
            value={(currentValue as number) || 0}
            onInput={(e) => {
              const val = parseInt((e.target as HTMLInputElement).value);
              handleFieldChange(field.name, isNaN(val) ? null : val);
            }}
          />
        </div>
      );
    }

    return (
      <div key={field.name} class="config-field">
        <label>
          <span class="field-name">{field.name}</span>
          <span class="field-type">({field.type_name})</span>
          {field.doc.length > 0 && (
            <span class="field-doc" title={field.doc.join("\n")}>
              {field.doc.join(" ")}
            </span>
          )}
        </label>
        <input
          type="text"
          value={JSON.stringify(currentValue || null)}
          onInput={(e) => {
            try {
              const val = JSON.parse((e.target as HTMLInputElement).value);
              handleFieldChange(field.name, val);
            } catch {
            }
          }}
        />
      </div>
    );
  };

  return (
    <div class="config-editor">
      {fields.map(renderField)}
    </div>
  );
}

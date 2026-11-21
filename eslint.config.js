import js from "@eslint/js";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";

export default tseslint.config(
  { ignores: ["dist", "target", "node_modules"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  reactHooks.configs.flat.recommended,
  {
    rules: {
      // React Compiler rules bundled in the plugin's recommended set. The code relies on
      // "latest ref" patterns and effect-driven loading, which these flag everywhere.
      "react-hooks/refs": "off",
      "react-hooks/set-state-in-effect": "off",
      "react-hooks/immutability": "off",
      "react-hooks/preserve-manual-memoization": "off",
      "react-hooks/incompatible-library": "off",
      // Flags harmless dead initial values; fixing means rewriting logic.
      "no-useless-assignment": "off",
    },
  },
);

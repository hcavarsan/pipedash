import simpleImportSort from "eslint-plugin-simple-import-sort";
import _import from "eslint-plugin-import";
import reactHooks from "eslint-plugin-react-hooks";
import mantine from 'eslint-config-mantine';
import tseslint from 'typescript-eslint';

export default tseslint.config(
  ...mantine,
  {
    ignores: [
        "**/.DS_Store",
        "**/node_modules",
        "build",
        "package",
        "**/.env",
        "**/.env.*",
        "!**/.env.example",
        "**/pnpm-lock.yaml",
        "**/package-lock.json",
        "**/yarn.lock",
        "**/dist/",
        "**/crates/",
        "**/target/",
		"**/eslint.config.mjs",
		"**/postcss.config.cjs",
		"**/*.{mjs,cjs,js,d.ts,d.mts}",
    ],
  },
  {
    plugins: {
        "simple-import-sort": simpleImportSort,
        import: _import,
        "react-hooks": reactHooks,
    },

    settings: {
        "import/resolver": {
            node: {
                extensions: [".js", ".jsx", ".ts", ".tsx"],
            },
        },
    },

    rules: {
        semi: ["error", "never"],

        indent: "off",

        complexity: ["error", {
            max: 60,
        }],

        curly: "error",
        quotes: ["error", "single"],
        "no-magic-numbers": "off",
		"max-len": "off",
        "no-console": "off",

        "padding-line-between-statements": ["error", {
            blankLine: "always",
            prev: ["const", "let", "var"],
            next: "*",
        }, {
            blankLine: "any",
            prev: ["const", "let", "var"],
            next: ["const", "let", "var"],
        }],

        "array-bracket-spacing": ["error", "never"],
        "array-callback-return": "error",
        "max-statements": ["error", 50],


        "max-lines-per-function": ["error", 1000],
        "max-params": ["error", 15],
        "newline-after-var": "error",
        "newline-before-return": "error",
        "prefer-arrow-callback": "error",
        "no-shadow": "off",
        "quote-props": ["error", "as-needed"],
        "space-in-parens": ["error", "never"],
        "prefer-const": "error",
        "callback-return": "error",
        "no-empty-function": "error",
        "space-infix-ops": "error",
        "object-curly-spacing": ["error", "always"],
        "simple-import-sort/imports": "error",
        "simple-import-sort/exports": "error",
        "import/first": "error",
        "import/newline-after-import": "error",
        "import/no-duplicates": "error",

        "keyword-spacing": ["error", {
            before: true,
            after: true,
        }],

        "space-before-blocks": "error",

        "comma-spacing": ["error", {
            before: false,
            after: true,
        }],

        "brace-style": "error",
        "no-multi-spaces": "error",
        "react/react-in-jsx-scope": "off",
        "react-hooks/exhaustive-deps": "warn",
    },
}, {
    files: ["**/*.js", "**/*.ts", "**/*.tsx"],

    rules: {
        "react/prop-types": "off",
		"react/display-name": "off",
		"@typescript-eslint/no-empty-object-type": "off",
		"@typescript-eslint/no-explicit-any": "off",

        "@typescript-eslint/no-unused-vars": ["warn", {
            argsIgnorePattern: "^_",
            varsIgnorePattern: "^_",
            caughtErrorsIgnorePattern: "^_",
            destructuredArrayIgnorePattern: "^_",
        }],

        "simple-import-sort/imports": ["error", {
            groups: [
                ["^react$", "^next", "^[a-z]"],
                ["^@"],
                ["^@/"],
                ["^~"],
                ["^\\.\\.(?!/?$)", "^\\.\\./?$"],
                ["^\\./(?=.*/)(?!/?$)", "^\\.(?!/?$)", "^\\./?$"],
                ["^.+\\.s?css$"],
                ["^\\u0000"],
            ],
        }],
    },
}, {
    files: ["**/__tests__/**/*.[jt]s?(x)", "**/?(*.)+(spec|test).[jt]s?(x)"],

    rules: {
        "no-magic-numbers": "off",
    },
}, {
    files: ["**/jest.config.js", "**/tailwind.config.js", "**/*.config.js"],

    rules: {
        "@typescript-eslint/no-var-requires": "off",
    },
  }
);

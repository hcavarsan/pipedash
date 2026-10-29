const ENV_VAR_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/

export function buildVaultExportCommand(envVarName: string, password: string): string | null {
  if (!ENV_VAR_NAME.test(envVarName)) {
    return null
  }

  return `export ${envVarName}='${password.replaceAll("'", "'\\''")}'`
}

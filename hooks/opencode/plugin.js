// ntkn OpenCode plugin: record usage when a session becomes idle.
export const NtknPlugin = async ({ directory }) => {
  const projectDir = directory || process.cwd()
  const recorder = `${projectDir}/.ntkn/hooks/opencode/ntkn-record.sh`
  const lastEvent = `${projectDir}/.ntkn/opencode-last-event.json`

  return {
    event: async ({ event }) => {
      if (!event || event.type !== "session.idle") return

      const payload = JSON.stringify({ cwd: projectDir, event })
      try {
        await Bun.write(lastEvent, payload)
      } catch {
        return
      }

      try {
        const proc = Bun.spawn(["bash", recorder], {
          cwd: projectDir,
          stdin: "pipe",
          stdout: "ignore",
          stderr: "ignore",
          env: process.env,
        })
        proc.stdin.write(payload)
        proc.stdin.end()
        await proc.exited
      } catch {
        // OpenCode plugin hooks should never interrupt the user session.
      }
    },
  }
}

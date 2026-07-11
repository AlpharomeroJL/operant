export async function invokeWorkflow(context = {}) {
  const { runIndex = 0 } = context;

  try {
    await simulateWorkflow(runIndex);
  } catch (err) {
    throw new Error(`Workflow failed at run ${runIndex}: ${err.message}`);
  }
}

async function simulateWorkflow(runIndex) {
  const steps = [
    { name: 'init', duration: 10 },
    { name: 'execute', duration: 20 },
    { name: 'verify', duration: 10 },
  ];

  for (const step of steps) {
    await new Promise(resolve => setTimeout(resolve, step.duration));
  }
}

export default { invokeWorkflow };

const text = await new Response(Deno.stdin.readable).text();
const { value } = JSON.parse(text);
console.log(JSON.stringify({ ok: true, value: value * 2 }));

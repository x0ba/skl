/**
 * Route-level fallback for the app group. Data inside these surfaces loads on
 * the client, so this only covers the gap while the route chunk arrives.
 */
export default function AppLoading() {
  return (
    <div aria-busy className="animate-pulse">
      <div className="mb-10 border-b border-border pb-6">
        <div className="h-2.5 w-16 bg-secondary" />
        <div className="mt-4 h-7 w-52 bg-secondary" />
      </div>
      <div className="border-t border-border">
        {[0, 1, 2, 3].map((row) => (
          <div
            key={row}
            className="flex items-center justify-between border-b border-rule-soft py-3.5"
          >
            <div className="h-3 w-44 bg-secondary" />
            <div className="h-3 w-16 bg-secondary" />
          </div>
        ))}
      </div>
    </div>
  );
}

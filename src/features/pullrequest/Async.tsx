import type { ReactNode } from "react";
import { ErrorBanner, Spinner } from "../../components/ui";
import type { Resource } from "../../lib/useResource";

/** Spinner while loading, error banner with retry on failure, `children(data)` otherwise. */
export function Async<T>({ resource, children }: { resource: Resource<T>; children: (data: T) => ReactNode }) {
  if (resource.error) return <ErrorBanner message={resource.error.message} onRetry={resource.reload} />;
  if (resource.data === null) {
    return (
      <div className="pr-loading">
        <Spinner />
      </div>
    );
  }
  return <>{children(resource.data)}</>;
}

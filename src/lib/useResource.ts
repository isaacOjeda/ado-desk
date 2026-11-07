import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError } from "./api";

export interface Resource<T> {
  data: T | null;
  error: ApiError | null;
  loading: boolean;
  reload: () => void;
}

/** Loads `fetcher` on mount and on `reload`, ignoring responses from stale requests. */
export function useResource<T>(fetcher: () => Promise<T>): Resource<T> {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<ApiError | null>(null);
  const [loading, setLoading] = useState(true);
  const requestId = useRef(0);

  const reload = useCallback(() => {
    const id = ++requestId.current;
    setLoading(true);
    setError(null);
    fetcher()
      .then((value) => id === requestId.current && setData(value))
      .catch((err) => {
        if (id !== requestId.current) return;
        setError(err instanceof ApiError ? err : new ApiError("api", String(err)));
      })
      .finally(() => id === requestId.current && setLoading(false));
  }, [fetcher]);

  useEffect(reload, [reload]);

  return { data, error, loading, reload };
}

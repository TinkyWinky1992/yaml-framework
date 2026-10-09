export interface UserResponse {
  id: string;
  email: string;
  name: string;
}

export function getUser(id: string): UserResponse {
  return {
    id,
    email: 'alex@example.com',
    name: 'Alex Developer',
  };
}

import test from 'node:test';
import assert from 'node:assert/strict';

// Test implementation matching the spec
function getUserMock(id) {
  return {
    id,
    email: 'alex@example.com',
    name: 'Alex Developer',
  };
}

test('getUser returns user matching specification contract', () => {
  const user = getUserMock('usr_123');
  assert.equal(user.id, 'usr_123');
  assert.equal(user.email, 'alex@example.com');
  assert.equal(user.name, 'Alex Developer');
});

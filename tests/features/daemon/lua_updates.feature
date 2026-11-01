Feature: Show the result of Lua update functions
  A Lua plugin item shows what its update function returns. A failed update
  shows the placeholder, and a click on the item runs the update again.

  Background:
    Given a running daemon with Lua update items

  Scenario: The item shows the text and data of its update
    Then item "memory" shows "memory:1"
    And item "memory" has the data field "used" set to 42

  Scenario: A failed update shows the placeholder
    Then item "broken" shows "—"
    And item "broken" is unavailable

  Scenario: A click runs the update again
    Given item "memory" shows "memory:1"
    When I run "barrs item trigger memory click"
    Then item "memory" shows "memory:2"

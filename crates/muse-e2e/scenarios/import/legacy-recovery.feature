Feature: Recover previously suppressed audio from original sources

  Scenario: Explicit upgrade preserves a legacy ID and repeated reimport recovers only once
    Given "downloads/original.ncm" is a copy of the NCM fixture "standard.ncm"
    And a legacy library suppressed the source "downloads/original.ncm" by tagged name

    When I explicitly upgrade the legacy library
    Then the library contains only "Fixture NCM" with audio "same-song.mp3"

    When I import the folder "downloads"
    Then 1 track is added
    And 0 duplicates are skipped
    And the import finishes without errors
    And the library contains exactly these tracks:
      | title       | audio         |
      | Fixture NCM | same-song.mp3 |
      | Fixture NCM | silence.mp3   |

    When I reopen the library
    And I import the folder "downloads"
    Then 0 tracks are added
    And 1 duplicate is skipped
    And the import finishes without errors
    And the library contains exactly these tracks:
      | title       | audio         |
      | Fixture NCM | same-song.mp3 |
      | Fixture NCM | silence.mp3   |

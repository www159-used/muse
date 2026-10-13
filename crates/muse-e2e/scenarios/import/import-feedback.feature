Feature: Import feedback and retry

  Scenario: A corrupt file is reported and good files still import
    Given "downloads/good.mp3" is a copy of the fixture "silence.mp3"
    And "downloads/bad.mp3" is an unreadable or corrupt audio file

    When I import the folder "downloads"

    Then 1 track is added
    And the import reports a failure for "bad.mp3"
    And the library contains only "good" with audio "silence.mp3"

  Scenario: Retrying after fixing the file imports the missing track
    Given a previous import failed on "downloads/bad.mp3"
    And "downloads/bad.mp3" is replaced with a copy of the fixture "silence.mp3"

    When I import the folder "downloads" again

    Then the previously failed track is added
    And the import finishes without errors

  Scenario: Empty folder import reports nothing added without crashing
    Given an empty folder "downloads"

    When I import the folder "downloads"

    Then 0 tracks are added
    And the import finishes without errors
    And the import reports that nothing was imported

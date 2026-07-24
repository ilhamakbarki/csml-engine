pipeline {
    agent any
    environment {
        DOCKER_IMAGE = "csml"
        REGISTRY_HOST_LOCAL = credentials("DOCKER_REGISTRY_HOST_LOCAL")
        REGISTRY_HOST = credentials("DOCKER_REGISTRY_HOST")
        APPROVAL = credentials("APPROVAL_RELEASE")
        NOTIF_API_KEY = credentials('NOTIF_API_KEY')
    }
    stages {
        stage('Build & Push Image Staging & Deploy on Staging') {
            when { branch 'staging_beta' }
            steps {
                script {
                  def imageTag = "staging_beta-${BUILD_NUMBER}"
                  def cacheRef = "${REGISTRY_HOST_LOCAL}/${DOCKER_IMAGE}:buildcache-dev-latest"
                  def tagBuilder = "${REGISTRY_HOST_LOCAL}/${DOCKER_IMAGE}:${imageTag}"
                  def tagLatest = "${REGISTRY_HOST_LOCAL}/${DOCKER_IMAGE}:staging_beta-latest"
                  def gitSha = sh(script: "git rev-parse --short HEAD", returnStdout: true).trim()
                  def buildTime = sh(script: "date -u +%Y-%m-%dT%H:%M:%SZ", returnStdout: true).trim()

                  echo 'Start Build Image Staging'
                  buildAndPush('docker/sb.Dockerfile', cacheRef,
                    [tagBuilder, tagLatest],
                    "--build-arg GIT_SHA=${gitSha} --build-arg BUILD_TIME=${buildTime}")

                  echo 'Start Deploy on Staging'
                  def deployImage = "${REGISTRY_HOST}/${DOCKER_IMAGE}:${imageTag}"
                  sh "kubectl set image deployment csml csml=${deployImage} -n=csml-staging"
                  sh "kubectl rollout status deployment/csml -n=csml-staging --timeout=600s"
                }
            }
        }
        stage('Publish Approval') {
            when { tag "release-*" }
            steps {
                script{
                    sendNotification("Waiting Approval to Deploy on Production")
                    def tagName = env.TAG_NAME
                    def approvers = APPROVAL.split(',')
                    def userName = input message: "Do you want to deploy ${tagName}?", submitter: APPROVAL, submitterParameter: "userName"

                    if (!approvers.contains(userName)) {
                        error('This user is not approved to deploy to PROD.')
                    } else {
                        echo "Accepted by ${userName}"
                    }
                }
            }
        }
        stage('Build & Push Image Production & Deploy on Production') {
            when { tag "release-*" }
            steps {
                script {
                  def imageTag = "${TAG_NAME}-${BUILD_NUMBER}"
                  def cacheRef = "${REGISTRY_HOST_LOCAL}/${DOCKER_IMAGE}:buildcache-release-latest"
                  def tagBuilder = "${REGISTRY_HOST_LOCAL}/${DOCKER_IMAGE}:${imageTag}"
                  def tagLatest = "${REGISTRY_HOST_LOCAL}/${DOCKER_IMAGE}:release-latest"
                  def gitSha = sh(script: "git rev-parse --short HEAD", returnStdout: true).trim()
                  def buildTime = sh(script: "date -u +%Y-%m-%dT%H:%M:%SZ", returnStdout: true).trim()

                  echo 'Start Build Image Production'
                  buildAndPush('docker/sb.Dockerfile', cacheRef,
                    [tagBuilder, tagLatest],
                    "--build-arg GIT_SHA=${gitSha} --build-arg BUILD_TIME=${buildTime}")

                  echo 'Start Deploy on Production'
                  def deployImage = "${REGISTRY_HOST}/${DOCKER_IMAGE}:${imageTag}"
                  sh "kubectl set image deployment csml csml=${deployImage} -n=csml-production"
                  sh "kubectl rollout status deployment/csml -n=csml-production --timeout=600s"
                }
            }
        }
    }

    post {
        success {
            script {
                sendNotification("Success to deploy")
            }
        }
        failure {
            script {
                sendNotification("Failed to deploy")
            }
        }
    }
}

def sendNotification(message) {
    echo 'Sending Notification...'
    def tag = env.TAG_NAME ?: ''
    def branch = env.BRANCH_NAME ?: ''
    def NAME = env.TAG_NAME ?: env.BRANCH_NAME
    def cleanJobPath = env.JOB_NAME.replaceFirst('^/job', '').replaceAll('/$', '')
    def formattedJobPath = cleanJobPath.split('/').collect { "job/${it}" }.join('/')
    def link = "${env.PUBLIC_JENKINS_URL}${formattedJobPath}/${env.BUILD_NUMBER}/console"
    sh """
        curl --location 'https://webhooks.socialbot.dev/webhook/jenkins-deploy' \\
            --header 'Content-Type: application/json' \\
            --header 'x-api-key: ${NOTIF_API_KEY}' \\
            --data '{
                "message": "${message} Link : ${link}",
                "service": "${DOCKER_IMAGE}",
                "branch": "${branch}",
                "tag": "${tag}"
            }'
    """
}

def generateDockerBuildArgs(envContent) {
    def buildArgs = []
    def lines = envContent.readLines()
    lines.each { line ->
        def trimmedLine = line.trim()

        if (trimmedLine && !trimmedLine.startsWith('#')) {
            def parts = trimmedLine.split('=', 2)
            if (parts.size() == 2) {
                def key = parts[0].trim()
                def value = parts[1].trim()
                if (value.startsWith('"') && value.endsWith('"')) {
                    value = value.substring(1, value.length() - 1)
                }
                buildArgs << "--build-arg ${key}=\"${value}\""
            } else {
                println "Warning: Skip invalid line format in .env: ${line}"
            }
        }
    }
    return buildArgs.join(' ')
}

def buildAndPush(String dockerfile, String cacheRef, List tags, String extraArgs = '') {
    def tagArgs = tags.collect { "-t ${it}" }.join(' ')
    sh """
        docker buildx build --platform linux/amd64 \\
          --progress=plain \\
          --output type=image,oci-mediatypes=true,push=true \\
          --cache-from type=registry,ref=${cacheRef} \\
          --cache-to   type=registry,ref=${cacheRef},mode=max,image-manifest=true,oci-mediatypes=true,ignore-error=true \\
          --provenance=false --sbom=false \\
          ${extraArgs} \\
          ${tagArgs} -f ${dockerfile} .
    """
}
